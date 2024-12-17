use clap::{Parser, Subcommand};
use notifico_attachment::AttachmentPlugin;
use notifico_core::contact::RawContact;
use notifico_core::credentials::memory::MemoryCredentialStorage;
use notifico_core::credentials::RawCredential;
use notifico_core::engine::plugin::core::CorePlugin;
use notifico_core::engine::{AttachmentMetadata, Engine, EventContext};
use notifico_core::pipeline::event::{EventHandler, ProcessEventRequest, RecipientSelector};
use notifico_core::pipeline::executor::PipelineExecutor;
use notifico_core::pipeline::storage::SinglePipelineStorage;
use notifico_core::pipeline::Pipeline;
use notifico_core::recipient::Recipient;
use notifico_core::recorder::BaseRecorder;
use notifico_core::step::SerializedStep;
use notifico_core::transport::TransportRegistry;
use notifico_template::source::fs::FilesystemSource;
use notifico_template::{PreRenderedTemplate, TemplateSelector, Templater};
use notifico_transports::all_transports;
use serde_json::json;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, OnceLock};
use tokio::task::JoinSet;
use tracing::{debug, info};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};
use url::Url;
use uuid::Uuid;

const SINGLETON_CREDENTIAL_NAME: &str = "default";
const SINGLETON_EVENT_NAME: &str = "default";

static DEFAULT_TEMPLATE_DIR: OnceLock<PathBuf> = OnceLock::new();

#[derive(Parser, Debug)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Send a notification locally (simple syntax)
    Send {
        /// Credential string, transport-specific (refer to the documentation for specific transport)
        credential: String,
        /// Recipient(s), can be an email, phone number, or any other unique identifier
        /// in following format: "TYPE:VALUE"
        contacts: Vec<String>,
        /// Template object in JSON5 format (can be used without escaping)
        #[arg(short, long, required = true)]
        template: Vec<String>,
        #[arg(long, default_value_os_t = DEFAULT_TEMPLATE_DIR.get().unwrap().clone(), env = "NOTIFICO_TEMPLATE_DIR")]
        template_dir: PathBuf,
        #[arg(short, long)]
        attach: Vec<String>,
        #[arg(short, long)]
        context: Option<String>,
    },
    /// Send an event to remote Notifico Ingest API
    SendEvent {
        /// URL of the Notifico Ingest API
        #[arg(short, long)]
        ingest: Url,
        /// Event name
        event: String,
        /// Recipient in JSON5 format (can be used without escaping). Refer to documentation for recipient schema.
        #[arg(short, long)]
        recipient: Vec<String>,
        /// Context in JSON5 format (can be used without escaping).
        context: String,
    },
}

#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv();

    let project_dirs = directories::ProjectDirs::from("tech", "Notifico", "Notifico").unwrap();

    DEFAULT_TEMPLATE_DIR
        .set(project_dirs.data_dir().join("templates"))
        .unwrap();

    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "notificox=info,notifico_core=info,warn");
    }

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Command::Send {
            credential,
            contacts,
            template,
            attach,
            template_dir,
            context,
        } => {
            std::fs::create_dir_all(&template_dir).unwrap();

            let mut engine = Engine::new();
            let mut transport_registry = TransportRegistry::new();
            let recorder = Arc::new(BaseRecorder::new());

            let credential = RawCredential::from_str(&credential).unwrap();

            let credentials = {
                let mut credentials = MemoryCredentialStorage::default();
                credentials.add_credential(
                    Uuid::nil(),
                    SINGLETON_CREDENTIAL_NAME.to_string(),
                    credential.clone(),
                );
                Arc::new(credentials)
            };

            let attachment_plugin = Arc::new(AttachmentPlugin::new(true));
            engine.add_plugin(attachment_plugin.clone());

            for (engine_plugin, transport_plugin) in all_transports(
                credentials.clone(),
                recorder.clone(),
                attachment_plugin.clone(),
            ) {
                engine.add_plugin(engine_plugin);
                transport_registry.register(transport_plugin);
            }

            let pipeline = {
                let mut pipeline = Pipeline::default();

                // templates.load
                let mut templates: Vec<TemplateSelector> = vec![];
                for template in template {
                    match json5::from_str(&template) {
                        Ok(parts) => templates.push(TemplateSelector::Inline {
                            inline: PreRenderedTemplate { parts },
                        }),
                        Err(e) => {
                            debug!(
                                "Failed to parse inline template: {e}, trying to parse as a name"
                            );
                            templates.push(TemplateSelector::File {
                                file: template.clone(),
                            })
                        }
                    }
                }

                if !templates.is_empty() {
                    let step = json!({
                        "step": "templates.load",
                        "templates": templates,
                    });

                    let step = SerializedStep(step.as_object().cloned().unwrap());
                    pipeline.steps.push(step);
                }

                // attachment.attach
                if !attach.is_empty() {
                    let mut attachments: Vec<AttachmentMetadata> = vec![];

                    for attachment in attach {
                        let path = PathBuf::from_str(&attachment).unwrap();
                        let abspath = path.canonicalize().unwrap();

                        attachments.push(AttachmentMetadata {
                            url: Url::from_file_path(abspath).unwrap(),
                            file_name: None,
                        })
                    }

                    let step = json!({
                        "step": "attachment.attach",
                        "attachments": attachments,
                    });

                    let step = SerializedStep(step.as_object().cloned().unwrap());
                    pipeline.steps.push(step);
                }

                // <TRANSPORT>.send or similar
                let transport_name = credential.transport;
                let step = json!({
                    "step": transport_registry.get_step(&transport_name).unwrap(),
                    "credential": SINGLETON_CREDENTIAL_NAME
                });
                let step = SerializedStep(step.as_object().cloned().unwrap());
                pipeline.steps.push(step);

                pipeline
            };

            info!(
                "Running pipeline: {}",
                serde_json::to_string_pretty(&pipeline).unwrap()
            );

            let contacts: Vec<RawContact> = contacts.iter().map(|s| s.parse().unwrap()).collect();

            let recipient = Recipient {
                id: Uuid::nil(),
                contacts,
            };

            let context: EventContext = context
                .map(|context| json5::from_str(&context).unwrap())
                .unwrap_or_default();

            let process_event_request = ProcessEventRequest {
                id: Uuid::nil(),
                project_id: Uuid::nil(),
                event: SINGLETON_EVENT_NAME.to_string(),
                recipients: vec![RecipientSelector::Recipient(recipient)],
                context,
            };

            let (pipelines_tx, pipelines_rx) = flume::unbounded();
            let pipelines_tx = Arc::new(pipelines_tx);

            engine.add_plugin(Arc::new(CorePlugin::new(pipelines_tx.clone())));

            let templater_source = Arc::new(FilesystemSource::new(template_dir));
            engine.add_plugin(Arc::new(Templater::new(templater_source.clone())));

            // Create PipelineExecutor
            let executor = Arc::new(PipelineExecutor::new(engine));
            let pipelines = Arc::new(SinglePipelineStorage::new(pipeline.clone()));
            let event_handler = EventHandler::new(pipelines.clone(), pipelines_tx.clone());

            event_handler
                .process_eventrequest(process_event_request)
                .await
                .unwrap();

            let mut joinset = JoinSet::new();
            loop {
                tokio::select! {
                    biased;
                    Ok(task) = pipelines_rx.recv_async() => {
                        let executor = executor.clone();
                        let _handle = joinset.spawn(async move {executor.execute_pipeline(serde_json::from_str(&task).unwrap()).await});
                    }
                    result = joinset.join_next() => {
                        if result.is_none() { break;}
                    },
                }
            }
        }
        Command::SendEvent {
            ingest,
            event,
            recipient,
            context,
        } => {
            let recipients: Vec<RecipientSelector> = recipient
                .iter()
                .map(|s| json5::from_str(s).unwrap())
                .collect();

            let request = ProcessEventRequest {
                id: Uuid::now_v7(),
                project_id: Default::default(),
                event,
                recipients,
                context: json5::from_str(&context).unwrap(),
            };

            let url = ingest.join("/v1/send").unwrap();

            let client = reqwest::Client::new();
            client.post(url).json(&request).send().await.unwrap();
        }
    }
}
