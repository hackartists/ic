#![allow(dead_code)]

use crate::canister_state::queues::REQUEST_LIFETIME;
use ic_types::messages::{Request, RequestOrResponse, Response, NO_DEADLINE};
use ic_types::time::CoarseTime;
use ic_types::{CountBytes, Time};
use phantom_newtype::Id;
use std::cmp::Reverse;
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BinaryHeap};
use std::sync::Arc;

#[cfg(test)]
mod tests;

pub struct MessageIdTag;
/// A generated identifier for messages held in a `MessagePool`.
pub type MessageId = Id<MessageIdTag, u64>;

/// A reference into a `MessagePool` that differentiates between requests and
/// responses.
pub(super) enum MessagePoolReference {
    /// Reference to a `Request` held in the message pool.
    Request(MessageId),

    /// Reference to a `Response` held in the message pool.
    Response(MessageId),
}

impl MessagePoolReference {
    /// Returns the ID within the reference.
    pub fn id(&self) -> MessageId {
        match self {
            Self::Request(id) => *id,
            Self::Response(id) => *id,
        }
    }
}

/// A placeholder for a potential late inbound best-effort response.
///
/// Does not implement `Clone` or `Copy` to ensure that it can only be used
/// once.
pub(super) struct ResponsePlaceholder(MessageId);

impl ResponsePlaceholder {
    /// Returns the message ID within.
    pub(super) fn id(&self) -> MessageId {
        self.0
    }
}

/// A pool of canister messages, guaranteed response and best effort, with
/// built-in support for time-based expiration and load shedding.
///
/// Messages in the pool are identified by a `MessageId` generated by the pool.
/// They can be retrieved or removed by ID or by reference (kind plus ID; e.g.
/// "request with ID 5").
///
/// Messages are added to the deadline queue based on their class (best-effort
/// vs guaranteed response) and context: i.e. all best-effort messages except
/// responses in input queues; plus guaranteed response call requests in output
/// queues. All best-effort messages (and only best-effort messages) are added
/// to the load shedding queue.
///
/// All pool operations except `expire_messages()` execute in at most
/// `O(log(N))` time.
#[derive(Clone, Debug)]
pub struct MessagePool {
    /// Pool contents.
    messages: BTreeMap<MessageId, RequestOrResponse>,

    /// Total size of all messages in the pool, in bytes.
    size_bytes: usize,

    /// Deadline priority queue, earliest deadlines first.
    ///
    /// Message IDs break ties, ensuring deterministic representation across
    /// replicas.
    deadline_queue: BinaryHeap<(Reverse<CoarseTime>, MessageId)>,

    /// Load shedding priority queue: largest message first.
    ///
    /// Message IDs break ties, ensuring deterministic representation across
    /// replicas.
    size_queue: BinaryHeap<(usize, MessageId)>,

    /// The ID to be assigned to the next message. Bumped every time a new message
    /// ID is assigned.
    next_message_id: MessageId,
}

impl MessagePool {
    /// Inserts an inbound message (one that is to be enqueued in an input queue)
    /// into the pool. Returns the ID assigned to the message.
    ///
    /// The message is added to the deadline queue iff it is a best-effort request
    /// (best effort responses that already made it into an input queue should not
    /// expire). It is added to the load shedding queue if it is a best-effort
    /// message.
    pub(crate) fn insert_inbound(&mut self, msg: RequestOrResponse) -> MessageId {
        let deadline = match &msg {
            RequestOrResponse::Request(request) => request.deadline,

            // Never expire responses already enqueued in an input queue.
            RequestOrResponse::Response(_) => NO_DEADLINE,
        };

        self.insert_impl(msg, deadline)
    }

    /// Inserts an outbound request (one that is to be enqueued in an output queue)
    /// into the pool. Returns the ID assigned to the request.
    ///
    /// The request is always added to the deadline queue: if it is a best-effort
    /// request, with its explicit deadline; if it is a guaranteed response call
    /// request, with a deadline of `now + REQUEST_LIFETIME`. It is added to the
    /// load shedding queue iff it is a best-effort request.
    pub(crate) fn insert_outbound_request(
        &mut self,
        request: Arc<Request>,
        now: Time,
    ) -> MessageId {
        let deadline = if request.deadline == NO_DEADLINE {
            // Guaranteed response call requests in canister output queues expire after
            // `REQUEST_LIFETIME`.
            CoarseTime::floor(now + REQUEST_LIFETIME)
        } else {
            // Best-effort requests expire as per their specidied deadline.
            request.deadline
        };

        self.insert_impl(RequestOrResponse::Request(request), deadline)
    }

    /// Inserts an outbound response (one that is to be enqueued in an output queue)
    /// into the pool. Returns the ID assigned to the response.
    ///
    /// The response is added to both the deadline queue and the load shedding queue
    /// iff it is a best-effort response.
    pub(crate) fn insert_outbound_response(&mut self, response: Arc<Response>) -> MessageId {
        let deadline = response.deadline;
        self.insert_impl(RequestOrResponse::Response(response), deadline)
    }

    /// Inserts the given message into the pool with the provided `deadline` (rather
    /// than the message's actual deadline; this is so we can expire the outgoing
    /// requests of guaranteed response calls; and not expire incoming best-effort
    /// responses). Returns the ID assigned to the message.
    ///
    /// The message is recorded into the deadline queue with the provided `deadline`
    /// iff that is non-zero. It is recorded in the load shedding priority queue iff
    /// the message is a best-effort message.
    fn insert_impl(&mut self, msg: RequestOrResponse, deadline: CoarseTime) -> MessageId {
        let id = self.next_message_id();
        let size_bytes = msg.count_bytes();
        let is_best_effort = msg.is_best_effort();

        // Insert.
        assert!(self.messages.insert(id, msg).is_none());

        // Update pool byte size.
        self.size_bytes += size_bytes;
        debug_assert_eq!(self.calculate_size_bytes(), self.size_bytes);

        // Record in deadline queue iff a deadline was provided.
        if deadline != NO_DEADLINE {
            self.deadline_queue.push((Reverse(deadline), id));
        }

        // Record in load shedding queue iff it's a best-effort message.
        if is_best_effort {
            self.size_queue.push((size_bytes, id));
        }

        id
    }

    /// Prepares a placeholder for a potential late inbound best-effort response.
    pub(super) fn insert_inbound_timeout_response(&mut self) -> ResponsePlaceholder {
        ResponsePlaceholder(self.next_message_id())
    }

    /// Inserts a late inbound best-effort response into a placeholder.
    pub(super) fn replace_inbound_timeout_response(
        &mut self,
        placeholder: ResponsePlaceholder,
        msg: RequestOrResponse,
    ) {
        // Message must be a best-effort response.
        match &msg {
            RequestOrResponse::Response(rep) if rep.deadline != NO_DEADLINE => {}
            _ => panic!("Message must be a best-effort response"),
        }

        let id = placeholder.0;
        let size_bytes = msg.count_bytes();

        // Insert. Cannot lead to a conflict because the placeholder is consumed on use.
        assert!(self.messages.insert(id, msg).is_none());

        // Update pool byte size.
        self.size_bytes += size_bytes;
        debug_assert_eq!(self.calculate_size_bytes(), self.size_bytes);

        // Record in load shedding queue only.
        self.size_queue.push((size_bytes, id));
    }

    /// Reserves and returns a new message ID.
    fn next_message_id(&mut self) -> MessageId {
        let id = self.next_message_id;
        self.next_message_id = (self.next_message_id.get() + 1).into();
        id
    }

    /// Retrieves the request with the given `MessageId`. Returns `None` if there is
    /// no message with the given ID in the pool; of if it's a response.
    pub(crate) fn get_request(&self, id: MessageId) -> Option<&RequestOrResponse> {
        match self.messages.get(&id) {
            request @ Some(RequestOrResponse::Request(_)) => request,
            Some(RequestOrResponse::Response(_)) | None => None,
        }
    }

    /// Retrieves the response with the given `MessageId`. Returns `None` if there
    /// is no message with the given ID in the pool; of if it's a request.
    pub(crate) fn get_response(&self, id: MessageId) -> Option<&RequestOrResponse> {
        match self.messages.get(&id) {
            response @ Some(RequestOrResponse::Response(_)) => response,
            Some(RequestOrResponse::Request(_)) | None => None,
        }
    }

    /// Retrieves the message identified by given reference.
    ///
    /// Returns `None` the conversion to `MessagePoolReference` fails; if no
    /// message with the given ID is present in the pool; or if the message in the
    /// pool is of a different kind (request vs response).
    pub(crate) fn get<R>(&self, reference: R) -> Option<&RequestOrResponse>
    where
        R: TryInto<MessagePoolReference>,
    {
        use MessagePoolReference::*;

        match reference.try_into().ok()? {
            Request(id) => self.get_request(id),
            Response(id) => self.get_response(id),
        }
    }

    /// Removes the message identified by given reference from the pool.
    ///
    /// Returns `None` the conversion to `MessagePoolReference` fails; if no
    /// message with the given ID is present in the pool; or if the message in the
    /// pool is of a different kind (request vs response).
    ///
    /// Updates the stats; and prunes the priority queues if necessary.
    pub(crate) fn take<R>(&mut self, reference: R) -> Option<RequestOrResponse>
    where
        R: TryInto<MessagePoolReference>,
    {
        use MessagePoolReference::*;

        let reference = reference.try_into().ok()?;
        let id = match reference {
            Request(id) => id,
            Response(id) => id,
        };

        let entry = match self.messages.entry(id) {
            Entry::Occupied(entry) => entry,
            Entry::Vacant(_) => return None,
        };

        let msg = match (reference, entry.get()) {
            (Request(_), RequestOrResponse::Request(_))
            | (Response(_), RequestOrResponse::Response(_)) => entry.remove(),

            (Request(_), RequestOrResponse::Response(_))
            | (Response(_), RequestOrResponse::Request(_)) => return None,
        };

        self.size_bytes -= msg.count_bytes();
        debug_assert_eq!(self.calculate_size_bytes(), self.size_bytes);
        self.maybe_trim_queues();

        Some(msg)
    }

    /// Removes the message with given ID from the pool.
    ///
    /// Updates the stats but does not prune the priority queues.
    fn take_by_id(&mut self, id: MessageId) -> Option<RequestOrResponse> {
        let msg = self.messages.remove(&id)?;

        self.size_bytes -= msg.count_bytes();
        debug_assert_eq!(self.calculate_size_bytes(), self.size_bytes);

        Some(msg)
    }

    /// Queries whether the deadline of any message in the pool has expired.
    pub(crate) fn has_expired_deadlines(&self, now: Time) -> bool {
        if let Some((deadline, _)) = self.deadline_queue.peek() {
            let now = CoarseTime::floor(now);
            if deadline.0 < now {
                return true;
            }
        }
        false
    }

    /// Drops all messages with expired deadlines (i.e. `deadline < now`) and
    /// returns them.
    ///
    /// Time complexity: `O(|expired_messages| * log(self.len()))`
    pub(crate) fn expire_messages(&mut self, now: Time) -> Vec<(MessageId, RequestOrResponse)> {
        if self.deadline_queue.is_empty() {
            return Vec::new();
        }

        let now = CoarseTime::floor(now);
        let mut expired = Vec::new();
        while let Some((deadline, id)) = self.deadline_queue.peek() {
            if deadline.0 >= now {
                break;
            }
            let id = *id;

            // Pop the deadline queue entry.
            self.deadline_queue.pop();

            // Drop the message, if present.
            if let Some(msg) = self.take_by_id(id) {
                expired.push((id, msg))
            }
        }

        self.maybe_trim_queues();

        expired
    }

    /// Drops the largest message in the pool and returns it.
    pub(crate) fn shed_largest_message(&mut self) -> Option<(MessageId, RequestOrResponse)> {
        // Keep trying until we actually drop a message.
        while let Some((_, id)) = self.size_queue.pop() {
            if let Some(msg) = self.take_by_id(id) {
                // A message was shed, prune the queues and return it.
                self.maybe_trim_queues();
                return Some((id, msg));
            }
        }

        // Nothing to shed.
        None
    }

    /// Returns the number of messages in the pool.
    pub(crate) fn len(&self) -> usize {
        self.messages.len()
    }

    /// Prunes stale entries from the priority queues if they make up more than half
    /// of the respective priority queue. This ensures amortized constant time.
    fn maybe_trim_queues(&mut self) {
        let len = self.messages.len();

        if self.deadline_queue.len() > 2 * len + 2 {
            self.deadline_queue
                .retain(|&(_, id)| self.messages.contains_key(&id));
        }
        if self.size_queue.len() > 2 * len + 2 {
            self.size_queue
                .retain(|&(_, id)| self.messages.contains_key(&id));
        }
    }

    /// Computes `size_bytes` from scratch. Used when deserializing and in
    /// `debug_assert!()` checks.
    ///
    /// Time complexity: `O(N)`.
    fn calculate_size_bytes(&self) -> usize {
        self.messages
            .values()
            .map(|message| message.count_bytes())
            .sum()
    }
}

impl PartialEq for MessagePool {
    fn eq(&self, other: &Self) -> bool {
        let Self {
            messages,
            size_bytes,
            deadline_queue,
            size_queue,
            next_message_id,
        } = self;
        let Self {
            messages: other_messages,
            size_bytes: other_size_bytes,
            deadline_queue: other_deadline_queue,
            size_queue: other_size_queue,
            next_message_id: other_next_message_id,
        } = other;

        messages == other_messages
            && size_bytes == other_size_bytes
            && deadline_queue.len() == other_deadline_queue.len()
            && deadline_queue
                .iter()
                .zip(other_deadline_queue.iter())
                .all(|(entry, other_entry)| entry == other_entry)
            && size_queue.len() == other_size_queue.len()
            && size_queue
                .iter()
                .zip(other_size_queue.iter())
                .all(|(entry, other_entry)| entry == other_entry)
            && next_message_id == other_next_message_id
    }
}
impl Eq for MessagePool {}

impl Default for MessagePool {
    fn default() -> Self {
        Self {
            messages: Default::default(),
            size_bytes: Default::default(),
            deadline_queue: Default::default(),
            size_queue: Default::default(),
            next_message_id: 0.into(),
        }
    }
}
