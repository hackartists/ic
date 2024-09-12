use crate::state::{FetchGuardError, FetchTxStatus, FetchedTx};
use crate::types::CheckTransactionResponse;
use crate::{blocklist_contains, GetTxError};
use bitcoin::{Address, Network, Transaction};
use futures::future::try_join_all;
use ic_btc_interface::Txid;
use std::convert::Infallible;

#[cfg(test)]
mod tests;

pub fn get_tx_cycle_cost(buffer_size: u32) -> u128 {
    // 1 KiB for request, buffer_size for response
    49_140_000 + 1024 * 5_200 + 10_400 * (buffer_size as u128)
}

/// Caller of check_transaction must attach this amount of cycles with the call.
pub const CHECK_TRANSACTION_CYCLES_REQUIRED: u128 = 40_000_000_000;

/// One-time charge for every check_transaction call.
pub const CHECK_TRANSACTION_CYCLES_SERVICE_FEE: u128 = 100_000_000;

// The response buffer size is initially set to 4kB, and then
// increased to 400kB if the initial size isn't enough.
// - The maximum size of a standard non-taproot transaction is 400k vBytes.
// - Taproot transactions could be as big as full block size (4MiB).
// - Currently a subnet's maximum response size is only 2MiB.
// - Transaction size between 400kB and 2MiB are also uncommon, we could
//   handle them in the future if required.
// - Transactions bigger than 2MiB are very rare, and we can't handle them.

/// Initial buffer size is 4kB
pub const INITIAL_BUFFER_SIZE: u32 = 4 * 1024;

/// Retry buffer size is 400kB
pub const RETRY_BUFFER_SIZE: u32 = 400 * 1024;

pub enum FetchResult {
    RetryWithBiggerBuffer,
    Error(GetTxError),
    Fetched(FetchedTx),
}

pub enum TryFetchResult<F> {
    Pending,
    HighLoad,
    Error(GetTxError),
    NotEnoughCycles,
    Fetched(FetchedTx),
    ToFetch(F),
}

/// Trait that abstracts over state operations.
pub trait FetchState {
    type FetchGuard;
    fn new_fetch_guard(&self, txid: Txid) -> Result<Self::FetchGuard, FetchGuardError>;
    fn get_fetch_status(&self, txid: Txid) -> Option<FetchTxStatus>;
    fn set_fetch_status(&self, txid: Txid, status: FetchTxStatus);
    fn set_fetched_address(&self, txid: Txid, index: usize, address: Address);
}

/// Trait that abstracts over system functions like fetching transaction, calcuating cycles, etc.
pub trait FetchEnv {
    async fn get_tx(&self, txid: Txid, buffer_size: u32) -> Result<Transaction, GetTxError>;
    fn cycles_accept(&self, cycles: u128) -> u128;
    fn cycles_available(&self) -> u128;

    /// Try to fetch a transaction given its txid:
    /// - If it is already available, return `Fetched`.
    /// - If it is already pending, return `Pending`.
    /// - If it is pending retry or not found, return a future that calls `fetch_tx`.
    /// - Or return other conditions like `HighLoad` or `Error`.
    fn try_fetch_tx<State: FetchState>(
        &self,
        state: &State,
        txid: Txid,
    ) -> TryFetchResult<impl futures::Future<Output = Result<FetchResult, Infallible>>> {
        let buffer_size = match state.get_fetch_status(txid) {
            None => INITIAL_BUFFER_SIZE,
            Some(FetchTxStatus::PendingRetry { buffer_size, .. }) => buffer_size,
            Some(FetchTxStatus::PendingOutcall { .. }) => return TryFetchResult::Pending,
            Some(FetchTxStatus::Error(msg)) => return TryFetchResult::Error(msg),
            Some(FetchTxStatus::Fetched(fetched)) => return TryFetchResult::Fetched(fetched),
        };
        let guard = match state.new_fetch_guard(txid) {
            Ok(guard) => guard,
            Err(_) => return TryFetchResult::HighLoad,
        };
        let cycle_cost = get_tx_cycle_cost(buffer_size);
        if self.cycles_accept(cycle_cost) < cycle_cost {
            TryFetchResult::NotEnoughCycles
        } else {
            TryFetchResult::ToFetch(self.fetch_tx(state, guard, txid, buffer_size))
        }
    }

    /// Fetch a transaction using http outcall by its txid and set its status to:
    /// - `Fetched`, if it is available.
    /// - `PendingRetry`, if the allocated buffer for outcall wasn't enough.
    /// - `Error`, if an irrecoverable error happened during the outcall of `get_tx`.
    ///
    /// Return the correponding `FetchResult`.
    ///
    /// Note that this function does not return any error, but due to requirements
    /// of `try_join_all` it must return a `Result` type.
    async fn fetch_tx<State: FetchState>(
        &self,
        state: &State,
        _guard: State::FetchGuard,
        txid: Txid,
        buffer_size: u32,
    ) -> Result<FetchResult, Infallible> {
        match self.get_tx(txid, buffer_size).await {
            Ok(tx) => {
                let input_addresses = tx.input.iter().map(|_| None).collect();
                let fetched = FetchedTx {
                    tx,
                    input_addresses,
                };
                state.set_fetch_status(txid, FetchTxStatus::Fetched(fetched.clone()));
                Ok(FetchResult::Fetched(fetched))
            }
            Err(GetTxError::ResponseTooLarge) if buffer_size < RETRY_BUFFER_SIZE => {
                state.set_fetch_status(
                    txid,
                    FetchTxStatus::PendingRetry {
                        buffer_size: RETRY_BUFFER_SIZE,
                    },
                );
                Ok(FetchResult::RetryWithBiggerBuffer)
            }
            Err(err) => {
                state.set_fetch_status(txid, FetchTxStatus::Error(err.clone()));
                Ok(FetchResult::Error(err))
            }
        }
    }

    /// After a transaction is successfully fetched, we still need to fetch
    /// all of its inputs in order to calculate input addresses. The steps
    /// are described as follows:
    /// - Fetch more if there are transaction inputs to be fetched and checked.
    /// - When they are done, calculate input addresses and record them.
    /// - For those failed due to insufficient outcall response buffer, mark their status
    ///   as `PendingRetry`.
    /// - If we are short of cycles and couldn't fetch all inputs, return `NotEnoughCycles`.
    /// - When all inputs are fetched, compute their addresses and return `Passed`
    ///   if all of them pass the check. Otherwise return `Failed`.
    ///
    /// Pre-condition: `txid` already exists in state with a `Fetched` status.
    async fn check_fetched<State: FetchState>(
        &self,
        state: &State,
        txid: Txid,
        fetched: &FetchedTx,
    ) -> CheckTransactionResponse {
        // Return Passed or Failed when all checks are complete, or None otherwise.
        fn check_completed(fetched: &FetchedTx) -> Option<CheckTransactionResponse> {
            if fetched.input_addresses.iter().all(|x| x.is_some()) {
                // We have obtained all input addresses.
                for address in fetched.input_addresses.iter().flatten() {
                    if blocklist_contains(address) {
                        return Some(CheckTransactionResponse::Failed);
                    }
                }
                Some(CheckTransactionResponse::Passed)
            } else {
                None
            }
        }

        if let Some(result) = check_completed(fetched) {
            return result;
        }

        let mut futures = vec![];
        let mut jobs = vec![];
        for (index, input) in fetched.tx.input.iter().enumerate() {
            if fetched.input_addresses[index].is_none() {
                use TryFetchResult::*;
                let input_txid = Txid::from(*(input.previous_output.txid.as_ref() as &[u8; 32]));
                match self.try_fetch_tx(state, input_txid) {
                    ToFetch(do_fetch) => {
                        jobs.push((index, input_txid, input.previous_output.vout));
                        futures.push(do_fetch)
                    }
                    Fetched(fetched) => {
                        let vout = input.previous_output.vout;
                        match transaction_output_address(&fetched.tx, vout) {
                            Ok(address) => state.set_fetched_address(txid, index, address),
                            Err(err) => {
                                return CheckTransactionResponse::Error(format!(
                                    "Error in fetching {}: {:?}",
                                    input_txid, err
                                ));
                            }
                        }
                    }
                    Pending => continue,
                    HighLoad | NotEnoughCycles | Error(_) => break,
                }
            }
        }

        if futures.is_empty() {
            // Return NotEnoughCycles if we have deducted all available cycles
            if self.cycles_available() == 0 {
                return CheckTransactionResponse::NotEnoughCycles;
            } else {
                return CheckTransactionResponse::HighLoad;
            }
        }

        let fetch_results = try_join_all(futures)
            .await
            .unwrap_or_else(|err| unreachable!("error in try_join_all {:?}", err));

        let mut error = None;
        for (i, result) in fetch_results.iter().enumerate() {
            match result {
                FetchResult::Fetched(fetched) => {
                    let (index, input_txid, vout) = jobs[i];
                    match transaction_output_address(&fetched.tx, vout) {
                        Ok(address) => state.set_fetched_address(txid, index, address),
                        Err(err) => {
                            error = Some(format!(
                                "error in computing address of {} vout {}: {:?}",
                                input_txid, vout, err
                            ))
                        }
                    }
                }
                FetchResult::Error(err) => {
                    error = Some(format!("error in fetching {}: {:?}", txid, err))
                }
                FetchResult::RetryWithBiggerBuffer => (),
            }
        }
        if let Some(err) = error {
            return CheckTransactionResponse::Error(err);
        }
        // Check again to see if we have completed
        match state
            .get_fetch_status(txid)
            .and_then(|result| match result {
                FetchTxStatus::Fetched(fetched) => check_completed(&fetched),
                _ => None,
            }) {
            Some(result) => result,
            None => CheckTransactionResponse::Pending,
        }
    }
}

fn transaction_output_address(tx: &Transaction, vout: u32) -> Result<Address, GetTxError> {
    let output = &tx.output[vout as usize];
    Address::from_script(&output.script_pubkey, Network::Bitcoin).map_err(GetTxError::Address)
}