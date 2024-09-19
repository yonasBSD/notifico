use crate::error::EngineError;
use crate::pipeline::SerializedStep;
use crate::recipient::Recipient;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::any::Any;
use std::borrow::Cow;
use std::collections::HashMap;

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EventContext(pub Map<String, Value>);

#[derive(Default, Debug)]
pub struct PipelineContext {
    pub recipient: Option<Recipient>,
    pub event_context: EventContext,
    pub plugin_contexts: HashMap<Cow<'static, str>, Value>,
}

#[async_trait]
pub trait EnginePlugin: Send + Sync + Any {
    async fn execute_step(
        &self,
        context: &mut PipelineContext,
        step: &SerializedStep,
    ) -> Result<(), EngineError>;

    fn step_namespace(&self) -> Cow<'static, str>;
}