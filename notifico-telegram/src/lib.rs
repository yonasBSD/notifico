use crate::step::STEPS;
use async_trait::async_trait;
use contact::TelegramContact;
use notifico_core::credentials::{get_typed_credential, Credentials, TypedCredential};
use notifico_core::engine::plugin::{EnginePlugin, StepOutput};
use notifico_core::engine::PipelineContext;
use notifico_core::error::EngineError;
use notifico_core::pipeline::SerializedStep;
use notifico_core::templater::RenderResponse;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::borrow::Cow;
use std::sync::Arc;
use step::Step;
use teloxide::prelude::Requester;
use teloxide::Bot;

mod contact;
mod step;

#[derive(Debug, Serialize, Deserialize)]
pub struct TelegramBotCredentials {
    token: String,
}

impl TypedCredential for TelegramBotCredentials {
    const CREDENTIAL_TYPE: &'static str = "telegram_token";
}

pub struct TelegramPlugin {
    credentials: Arc<dyn Credentials>,
}

impl TelegramPlugin {
    pub fn new(credentials: Arc<dyn Credentials>) -> Self {
        Self { credentials }
    }
}

#[async_trait]
impl EnginePlugin for TelegramPlugin {
    async fn execute_step(
        &self,
        context: &mut PipelineContext,
        step: &SerializedStep,
    ) -> Result<StepOutput, EngineError> {
        let step: Step = step.clone().convert_step()?;

        match step {
            Step::Send { credential } => {
                let Some(recipient) = context.recipient.clone() else {
                    return Err(EngineError::RecipientNotSet);
                };

                let credential: TelegramBotCredentials = get_typed_credential(
                    self.credentials.as_ref(),
                    context.project_id,
                    &credential,
                )
                .await?;
                let bot = Bot::new(credential.token);
                let contact: TelegramContact = recipient.get_primary_contact()?;

                for message in context.messages.iter().cloned() {
                    let content: TelegramContent = message.try_into().unwrap();

                    // Send
                    bot.send_message(contact.clone().into_recipient(), content.body)
                        .await
                        .unwrap();
                }
            }
        }

        Ok(StepOutput::Continue)
    }

    fn steps(&self) -> Vec<Cow<'static, str>> {
        STEPS.iter().map(|&s| s.into()).collect()
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TelegramContent {
    pub body: String,
}

impl TryFrom<RenderResponse> for TelegramContent {
    type Error = ();

    fn try_from(value: RenderResponse) -> Result<Self, Self::Error> {
        serde_json::from_value(Value::from_iter(value.0)).map_err(|_| ())
    }
}
