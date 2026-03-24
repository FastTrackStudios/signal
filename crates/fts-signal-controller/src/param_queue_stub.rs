//! Stub param queue for minimal builds (no crossbeam/signal-proto).

#[derive(Clone)]
pub struct ParamQueueProducer;

pub struct ParamQueueConsumer;

pub fn param_queue() -> (ParamQueueProducer, ParamQueueConsumer) {
    (ParamQueueProducer, ParamQueueConsumer)
}
