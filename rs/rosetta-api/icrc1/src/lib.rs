pub mod blocks;
pub mod endpoints;
pub mod hash;

use candid::Principal;
use ciborium::tag::Required;
use ic_base_types::PrincipalId;
use ic_ledger_canister_core::ledger::{LedgerContext, LedgerTransaction, TxApplyError};
use ic_ledger_core::{
    balances::Balances,
    block::{BlockType, EncodedBlock, FeeCollector, HashOf},
    timestamp::TimeStamp,
    tokens::Tokens,
};
use icrc_ledger_types::transaction::Memo;
use icrc_ledger_types::{Account, Subaccount};
use serde::ser::Error;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::collections::HashMap;
use std::convert::TryFrom;

fn ser_compact_account<S>(acc: &Account, s: S) -> Result<S::Ok, S::Error>
where
    S: serde::ser::Serializer,
{
    CompactAccount::from(*acc).serialize(s)
}

fn de_compact_account<'de, D>(d: D) -> Result<Account, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    use serde::de::Error;
    let compact_account = CompactAccount::deserialize(d)?;
    Account::try_from(compact_account).map_err(D::Error::custom)
}

fn ser_opt_compact_account<S>(acc: &Option<Account>, s: S) -> Result<S::Ok, S::Error>
where
    S: serde::ser::Serializer,
{
    acc.map_or_else(
        || Err(S::Error::custom("Expected some account but found None")),
        |acc| CompactAccount::from(acc).serialize(s),
    )
}

fn de_opt_compact_account<'de, D>(d: D) -> Result<Option<Account>, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    use serde::de::Error;
    let compact_account = CompactAccount::deserialize(d)?;
    let account = Account::try_from(compact_account).map_err(D::Error::custom)?;
    Ok(Some(account))
}

/// A compact representation of an Account.
///
/// Instead of encoding accounts as structs with named fields,
/// we encode them as tuples with variables number of elements.
/// ```text
/// [bytes] <=> Account { owner: bytes, subaccount : None }
/// [x: bytes, y: bytes] <=> Account { owner: x, subaccount: Some(y) }
/// ```
#[derive(Serialize, Deserialize)]
#[serde(transparent)]
pub struct CompactAccount(Vec<ByteBuf>);

impl From<Account> for CompactAccount {
    fn from(acc: Account) -> Self {
        let mut components = vec![ByteBuf::from(acc.owner.as_slice().to_vec())];
        if let Some(sub) = acc.subaccount {
            components.push(ByteBuf::from(sub.to_vec()))
        }
        CompactAccount(components)
    }
}

impl TryFrom<CompactAccount> for Account {
    type Error = String;
    fn try_from(compact: CompactAccount) -> Result<Account, String> {
        let elems = compact.0;
        if elems.is_empty() {
            return Err("account tuple must have at least one element".to_string());
        }
        if elems.len() > 2 {
            return Err(format!(
                "account tuple must have at most two elements, got {}",
                elems.len()
            ));
        }

        let principal =
            Principal::try_from(&elems[0][..]).map_err(|e| format!("invalid principal: {}", e))?;
        let subaccount = if elems.len() > 1 {
            Some(Subaccount::try_from(&elems[1][..]).map_err(|_| {
                format!(
                    "invalid subaccount: expected 32 bytes, got {}",
                    elems[1].len()
                )
            })?)
        } else {
            None
        };

        Ok(Account {
            owner: principal,
            subaccount,
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Hash, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(tag = "op")]
pub enum Operation {
    #[serde(rename = "mint")]
    Mint {
        #[serde(serialize_with = "ser_compact_account")]
        #[serde(deserialize_with = "de_compact_account")]
        to: Account,
        #[serde(rename = "amt")]
        amount: u64,
    },
    #[serde(rename = "xfer")]
    Transfer {
        #[serde(serialize_with = "ser_compact_account")]
        #[serde(deserialize_with = "de_compact_account")]
        from: Account,
        #[serde(serialize_with = "ser_compact_account")]
        #[serde(deserialize_with = "de_compact_account")]
        to: Account,
        #[serde(rename = "amt")]
        amount: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        fee: Option<u64>,
    },
    #[serde(rename = "burn")]
    Burn {
        #[serde(serialize_with = "ser_compact_account")]
        #[serde(deserialize_with = "de_compact_account")]
        from: Account,
        #[serde(rename = "amt")]
        amount: u64,
    },
}

#[derive(Serialize, Deserialize, Clone, Hash, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Transaction {
    #[serde(flatten)]
    pub operation: Operation,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "ts")]
    pub created_at_time: Option<u64>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<Memo>,
}

impl LedgerTransaction for Transaction {
    type AccountId = Account;
    type SpenderId = PrincipalId;

    fn burn(
        from: Account,
        amount: Tokens,
        created_at_time: Option<TimeStamp>,
        memo: Option<u64>,
    ) -> Self {
        Self {
            operation: Operation::Burn {
                from,
                amount: amount.get_e8s(),
            },
            created_at_time: created_at_time.map(|t| t.as_nanos_since_unix_epoch()),
            memo: memo.map(Memo::from),
        }
    }

    fn created_at_time(&self) -> Option<TimeStamp> {
        self.created_at_time
            .map(TimeStamp::from_nanos_since_unix_epoch)
    }

    fn hash(&self) -> HashOf<Self> {
        let mut cbor_bytes = vec![];
        ciborium::ser::into_writer(self, &mut cbor_bytes)
            .expect("bug: failed to encode a transaction");
        hash::hash_cbor(&cbor_bytes)
            .map(HashOf::new)
            .unwrap_or_else(|err| {
                panic!(
                    "bug: transaction CBOR {} is not hashable: {}",
                    hex::encode(&cbor_bytes),
                    err
                )
            })
    }

    fn apply<C>(
        &self,
        context: &mut C,
        _now: TimeStamp,
        effective_fee: Tokens,
    ) -> Result<(), TxApplyError>
    where
        C: LedgerContext<AccountId = Self::AccountId>,
    {
        let fee_collector = context.fee_collector().map(|fc| fc.fee_collector);
        let fee_collector = fee_collector.as_ref();
        match &self.operation {
            Operation::Transfer {
                from,
                to,
                amount,
                fee,
            } => context.balances_mut().transfer(
                from,
                to,
                Tokens::from_e8s(*amount),
                fee.map(Tokens::from_e8s).unwrap_or(effective_fee),
                fee_collector,
            )?,
            Operation::Burn { from, amount } => context
                .balances_mut()
                .burn(from, Tokens::from_e8s(*amount))?,
            Operation::Mint { to, amount } => {
                context.balances_mut().mint(to, Tokens::from_e8s(*amount))?
            }
        }
        Ok(())
    }
}

impl Transaction {
    pub fn mint(
        to: Account,
        amount: Tokens,
        created_at_time: Option<TimeStamp>,
        memo: Option<Memo>,
    ) -> Self {
        Self {
            operation: Operation::Mint {
                to,
                amount: amount.get_e8s(),
            },
            created_at_time: created_at_time.map(|t| t.as_nanos_since_unix_epoch()),
            memo,
        }
    }

    pub fn transfer(
        from: Account,
        to: Account,
        amount: Tokens,
        fee: Option<Tokens>,
        created_at_time: Option<TimeStamp>,
        memo: Option<Memo>,
    ) -> Self {
        Self {
            operation: Operation::Transfer {
                from,
                to,
                amount: amount.get_e8s(),
                fee: fee.map(Tokens::get_e8s),
            },
            created_at_time: created_at_time.map(|t| t.as_nanos_since_unix_epoch()),
            memo,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Hash, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Block {
    #[serde(rename = "phash")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_hash: Option<HashOf<EncodedBlock>>,
    #[serde(rename = "tx")]
    pub transaction: Transaction,
    #[serde(rename = "fee")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_fee: Option<u64>,
    #[serde(rename = "ts")]
    pub timestamp: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "fee_col")]
    #[serde(serialize_with = "ser_opt_compact_account")]
    #[serde(deserialize_with = "de_opt_compact_account")]
    pub fee_collector: Option<Account>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "fee_col_block")]
    pub fee_collector_block_index: Option<u64>,
}

type TaggedBlock = Required<Block, 55799>;

impl BlockType for Block {
    type Transaction = Transaction;
    type AccountId = Account;

    fn encode(self) -> EncodedBlock {
        let mut bytes = vec![];
        let value: TaggedBlock = Required(self);
        ciborium::ser::into_writer(&value, &mut bytes).expect("bug: failed to encode a block");
        EncodedBlock::from_vec(bytes)
    }

    fn decode(encoded_block: EncodedBlock) -> Result<Self, String> {
        let bytes = encoded_block.into_vec();
        let tagged_block: TaggedBlock = ciborium::de::from_reader(&bytes[..])
            .map_err(|e| format!("failed to decode a block: {}", e))?;
        Ok(tagged_block.0)
    }

    fn block_hash(encoded_block: &EncodedBlock) -> HashOf<EncodedBlock> {
        hash::hash_cbor(encoded_block.as_slice())
            .map(HashOf::new)
            .unwrap_or_else(|err| {
                panic!(
                    "bug: encoded block {} is not hashable cbor: {}",
                    hex::encode(encoded_block.as_slice()),
                    err
                )
            })
    }

    fn parent_hash(&self) -> Option<HashOf<EncodedBlock>> {
        self.parent_hash
    }

    fn timestamp(&self) -> TimeStamp {
        TimeStamp::from_nanos_since_unix_epoch(self.timestamp)
    }

    fn from_transaction(
        parent_hash: Option<HashOf<EncodedBlock>>,
        transaction: Self::Transaction,
        timestamp: TimeStamp,
        effective_fee: Tokens,
        fee_collector: Option<FeeCollector<Self::AccountId>>,
    ) -> Self {
        let effective_fee = if let Operation::Transfer { fee, .. } = &transaction.operation {
            fee.is_none().then_some(effective_fee.get_e8s())
        } else {
            None
        };
        let (fee_collector, fee_collector_block_index) = match fee_collector {
            Some(FeeCollector {
                fee_collector,
                block_index: None,
            }) => (Some(fee_collector), None),
            Some(FeeCollector { block_index, .. }) => (None, block_index),
            None => (None, None),
        };
        Self {
            parent_hash,
            transaction,
            effective_fee,
            timestamp: timestamp.as_nanos_since_unix_epoch(),
            fee_collector,
            fee_collector_block_index,
        }
    }
}

pub type LedgerBalances = Balances<HashMap<Account, Tokens>>;
