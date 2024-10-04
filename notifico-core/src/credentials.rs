use crate::error::EngineError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Credential {
    pub r#type: String,
    pub name: String,
    pub value: Value,
}

impl Credential {
    pub fn into_typed<T>(self) -> Result<T, EngineError>
    where
        T: TypedCredential,
    {
        if self.r#type != T::CREDENTIAL_TYPE {
            return Err(EngineError::InvalidCredentialFormat);
        }
        serde_json::from_value(self.value).map_err(|_| EngineError::InvalidCredentialFormat)
    }
}

pub trait TypedCredential: for<'de> Deserialize<'de> {
    const CREDENTIAL_TYPE: &'static str;
}

#[async_trait]
pub trait Credentials: Send + Sync {
    async fn get_credential(
        &self,
        project: Uuid,
        r#type: &str,
        name: &str,
    ) -> Result<Credential, EngineError>;
}

pub async fn get_typed_credential<T>(
    credentials: &dyn Credentials,
    project: Uuid,
    name: &str,
) -> Result<T, EngineError>
where
    T: TypedCredential,
{
    credentials
        .get_credential(project, T::CREDENTIAL_TYPE, name)
        .await
        .and_then(|c| c.into_typed())
}
