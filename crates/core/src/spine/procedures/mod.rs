//! Spine procedures — business logic that processes pipeline events.

pub mod commitment_detector;
pub mod history_recorder;
pub mod inbound_router;
pub mod model_invoker;
pub mod response_router;
pub mod thread_lifecycle;
pub mod thread_router_procedure;
pub mod threaded_delivery;
pub mod threaded_history_recorder;
pub mod tool_executor;
pub mod topic_thread_bridge;
