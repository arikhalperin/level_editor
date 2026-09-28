//! The generation dialog, and the worker thread behind it.
//!
//! Local inference is slow — minutes, on a machine with no GPU — so none of it may happen
//! on the UI thread. The run lives on a worker; the editor polls it each frame, keeps
//! repainting, and shows which stage it has reached. Cancelling sets a flag the run checks
//! between every request and every proof, so a cancelled generation stops without
//! delivering anything and without touching the open level.
//!
//! Everything here that can be decided without a screen — what the settings are, what the
//! progress line says, what the result report says — is a plain function, so it is checked
//! by ordinary tests rather than by looking at the window.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::time::Instant;

use egui::Vec2;

use crate::ai_client::{
    api_key_from_env, parse_endpoint, HttpModelClient, ModelClient, ModelSettings, API_KEY_ENV,
};
use crate::level_gen::{self, GenError, GenerationParams, Generated, Progress};

/// What the worker sends back.
enum Message {
    Stage(Progress),
    Finished(Box<Result<Generated, GenError>>),
}

/// A generation in flight.
pub struct GenerateRun {
    rx: Receiver<Message>,
    cancel: Arc<AtomicBool>,
    /// The stage most recently reported.
    pub stage: Progress,
    pub started: Instant,
    /// Set once the worker is done; `None` while it is still running.
    pub outcome: Option<Result<Generated, GenError>>,
}

impl GenerateRun {
    /// Start a run on a worker thread.
    pub fn start(mut client: Box<dyn ModelClient>, params: GenerationParams) -> Self {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        std::thread::spawn(move || {
            let stage_tx = tx.clone();
            let mut report = move |stage: Progress| {
                let _ = stage_tx.send(Message::Stage(stage));
            };
            let result =
                level_gen::generate(client.as_mut(), &params, &mut report, &worker_cancel);
            let _ = tx.send(Message::Finished(Box::new(result)));
        });
        Self {
            rx,
            cancel,
            stage: Progress::Contacting,
            started: Instant::now(),
            outcome: None,
        }
    }

    /// Take whatever the worker has said since last time. Returns true while it is still
    /// running, so the caller knows to keep repainting.
    pub fn poll(&mut self) -> bool {
        loop {
            match self.rx.try_recv() {
                Ok(Message::Stage(stage)) => self.stage = stage,
                Ok(Message::Finished(result)) => {
                    self.outcome = Some(*result);
                    return false;
                }
                Err(TryRecvError::Empty) => return self.outcome.is_none(),
                Err(TryRecvError::Disconnected) => {
                    if self.outcome.is_none() {
                        // The worker ended without sending a result. If it was asked to
                        // stop, that is a cancellation; otherwise it died, and saying
                        // "cancelled" would blame the user for a crash.
                        self.outcome = Some(Err(if self.cancel.load(Ordering::Relaxed) {
                            GenError::Cancelled
                        } else {
                            GenError::Worker
                        }));
                    }
                    return false;
                }
            }
        }
    }

    /// Ask the run to stop. It checks between every request and every proof.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// The line shown while a run is in flight.
pub fn stage_line(stage: &Progress, elapsed_secs: u64) -> String {
    let what = match stage {
        Progress::Contacting => "Contacting the model".to_string(),
        Progress::Outline => "Asking for the shape of the area".to_string(),
        Progress::Chamber { index, total, name } => {
            format!("Chamber {} of {}: {}", index + 1, total, name)
        }
        Progress::Proving => "Proving the route is playable".to_string(),
        Progress::Retrying { attempt, .. } => {
            format!("The route did not work; asking the model again ({attempt} of {})",
                level_gen::MAX_PROOF_RETRIES)
        }
        Progress::Repairing => "Repairing the geometry so the route closes".to_string(),
    };
    format!("{what} — {}", elapsed(elapsed_secs))
}

/// Elapsed time, in words that suit a wait of minutes.
pub fn elapsed(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else {
        format!("{}m {}s", secs / 60, secs % 60)
    }
}

/// What the user is told about a level that was delivered.
pub fn report(generated: &Generated) -> String {
    let chambers = generated.chambers.len();
    let entities = generated.level.entities.len();
    let mut text = format!(
        "{chambers} chamber(s), {entities} entities, and a route the play simulation walked \
         in {} move(s), taking {:.0} seconds to run.\n\n",
        generated.route_moves, generated.route_secs
    );
    if !generated.skipped.is_empty() {
        text.push_str(&format!(
            "{} thing(s) the model described too incompletely to place were left out:\n",
            generated.skipped.len()
        ));
        for note in &generated.skipped {
            text.push_str(&format!("  • {note}\n"));
        }
        text.push('\n');
    }
    if generated.is_the_models_own() {
        text.push_str("Every entity here is the model's own work; nothing was changed.");
    } else {
        text.push_str(&format!(
            "The model's route did not work, so {} ledge(s) were added to close it. \
             Everything else is the model's own work:\n",
            generated.repairs.len()
        ));
        for note in &generated.repairs {
            text.push_str(&format!("  • {note}\n"));
        }
    }
    text
}

/// The settings and prompt the dialog holds between openings.
#[derive(Clone, Debug, PartialEq)]
pub struct GenerateDialog {
    pub prompt: String,
    pub settings: ModelSettings,
    /// Held as text so a half-typed number does not fight the user.
    pub seed_text: String,
    pub chambers: usize,
    pub extent: Vec2,
}

impl Default for GenerateDialog {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            settings: ModelSettings::default(),
            seed_text: String::new(),
            chambers: 5,
            extent: Vec2::new(6000.0, 3000.0),
        }
    }
}

/// The range of chamber counts the dialog offers and the pipeline accepts.
pub const CHAMBERS_MIN: usize = 2;
pub const CHAMBERS_MAX: usize = 12;

/// Whether this endpoint needs a key, and whether one is to hand. Never the key itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyStatus {
    /// A local endpoint: no key is needed and none will be sent.
    NotNeeded,
    /// A key is needed and the environment has one.
    Present,
    /// A key is needed and there is none.
    Missing,
}

impl KeyStatus {
    /// What the dialog says about the key. It reports only the situation, never the key.
    pub fn line(self) -> String {
        match self {
            KeyStatus::NotNeeded => {
                "This endpoint is on this machine, so no API key is needed or sent.".to_string()
            }
            KeyStatus::Present => {
                format!("Using the API key from {API_KEY_ENV}.")
            }
            KeyStatus::Missing => format!(
                "{API_KEY_ENV} is not set. Set it in your shell and start the editor from there."
            ),
        }
    }
}

impl GenerateDialog {
    /// Put remembered settings into the dialog, including the seed box.
    ///
    /// The seed has to be written into `seed_text`, not just into `settings`: the text box is
    /// the only place `model_settings()` reads a seed from, so setting the struct field alone
    /// left the box blank after a restart, sent no seed, and then erased the remembered one
    /// on the next Generate.
    pub fn adopt_settings(&mut self, settings: ModelSettings) {
        self.seed_text = settings.seed.map(|s| s.to_string()).unwrap_or_default();
        self.settings = settings;
    }

    /// Whether this dialog's endpoint needs a key, and whether there is one.
    pub fn key_status(&self) -> KeyStatus {
        let needs = parse_endpoint(self.settings.endpoint.trim())
            .map(|e| e.needs_api_key())
            // An endpoint that does not parse is reported on when Generate is pressed;
            // assume the cautious answer until then.
            .unwrap_or(true);
        if !needs {
            KeyStatus::NotNeeded
        } else if api_key_from_env().is_some() {
            KeyStatus::Present
        } else {
            KeyStatus::Missing
        }
    }

    /// Whether `Generate` can be pressed: a model to ask, somewhere to ask it, and — where
    /// the endpoint needs one — a key, so a request that must fail is never started.
    pub fn can_generate(&self) -> bool {
        !self.settings.model.trim().is_empty()
            && !self.settings.endpoint.trim().is_empty()
            && self.key_status() != KeyStatus::Missing
    }

    /// Why `Generate` is unavailable, for the hover text.
    pub fn why_not_generate(&self) -> Option<String> {
        if self.settings.endpoint.trim().is_empty() {
            return Some("Enter the endpoint to ask".to_string());
        }
        if self.settings.model.trim().is_empty() {
            return Some("Enter the model to ask for".to_string());
        }
        if self.key_status() == KeyStatus::Missing {
            return Some(format!("{API_KEY_ENV} is not set in this editor's environment"));
        }
        None
    }

    /// The parameters a run would use.
    pub fn params(&self) -> GenerationParams {
        GenerationParams {
            prompt: self.prompt.clone(),
            extent: self.extent,
            // The same range the slider offers, so the two cannot disagree.
            chambers: self.chambers.clamp(CHAMBERS_MIN, CHAMBERS_MAX),
        }
    }

    /// The settings a run would use, with the seed read out of its text box.
    pub fn model_settings(&self) -> ModelSettings {
        ModelSettings {
            endpoint: self.settings.endpoint.trim().to_string(),
            model: self.settings.model.trim().to_string(),
            seed: self.seed_text.trim().parse().ok(),
        }
    }

    /// A client that would talk to the configured server.
    pub fn client(&self) -> Box<dyn ModelClient> {
        Box::new(HttpModelClient::new(self.model_settings()))
    }
}

/// Put the model settings into the editor's config document.
pub fn write_settings(config: &mut serde_json::Value, settings: &ModelSettings) {
    if !config.is_object() {
        *config = serde_json::json!({});
    }
    config["ai_endpoint"] = serde_json::Value::String(settings.endpoint.clone());
    config["ai_model"] = serde_json::Value::String(settings.model.clone());
    match settings.seed {
        Some(seed) => config["ai_seed"] = serde_json::json!(seed),
        None => {
            if let Some(map) = config.as_object_mut() {
                map.remove("ai_seed");
            }
        }
    }
}

/// Read them back, falling back to the defaults for anything absent or malformed — a config
/// written before this feature existed simply has none of these keys.
pub fn read_settings(config: &serde_json::Value) -> ModelSettings {
    let default = ModelSettings::default();
    ModelSettings {
        endpoint: config
            .get("ai_endpoint")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.to_string())
            .unwrap_or(default.endpoint),
        model: config
            .get("ai_model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or(default.model),
        seed: config.get("ai_seed").and_then(|v| v.as_u64()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_client::ScriptedClient;
    use crate::level_data::LevelData;

    // A2, second half — the settings survive a save and reload of the config.
    #[test]
    fn the_endpoint_and_model_survive_a_save_and_reload() {
        let settings = ModelSettings {
            endpoint: "http://192.168.1.50:1234/v1".into(),
            model: "qwen2.5-coder:14b".into(),
            seed: Some(99),
        };
        // Written into a config that already holds the editor's other remembered values,
        // which must not be disturbed.
        let mut config = serde_json::json!({ "last_level": "/tmp/level.json" });
        write_settings(&mut config, &settings);

        let text = serde_json::to_string(&config).expect("serialises");
        let reloaded: serde_json::Value = serde_json::from_str(&text).expect("deserialises");

        assert_eq!(read_settings(&reloaded), settings);
        assert_eq!(reloaded["last_level"], "/tmp/level.json", "the rest of the config is untouched");
    }

    // A19 — the defaults a fresh config yields.
    #[test]
    fn a_config_written_before_this_feature_existed_reads_as_the_openai_defaults() {
        let config = serde_json::json!({ "last_level": "/tmp/level.json" });
        let settings = read_settings(&config);
        assert_eq!(settings.endpoint, "https://api.openai.com/v1");
        assert_eq!(settings.model, "gpt-6-astra");
        assert_eq!(settings.seed, None);
    }

    #[test]
    fn a_remembered_local_endpoint_survives_and_needs_no_key() {
        let mut config = serde_json::json!({});
        write_settings(
            &mut config,
            &ModelSettings {
                endpoint: "http://localhost:11434/v1".into(),
                model: "llama3.1:8b".into(),
                seed: None,
            },
        );
        let settings = read_settings(&config);
        assert_eq!(settings.endpoint, "http://localhost:11434/v1");
        let dialog = GenerateDialog { settings, ..GenerateDialog::default() };
        assert_eq!(dialog.key_status(), KeyStatus::NotNeeded);
        assert!(dialog.can_generate(), "a local model needs no key to be generated with");
        assert!(dialog.key_status().line().contains("no API key is needed"));
    }

    #[test]
    fn clearing_the_seed_removes_it_from_the_config() {
        let mut config = serde_json::json!({});
        write_settings(&mut config, &ModelSettings { seed: Some(5), ..Default::default() });
        assert_eq!(read_settings(&config).seed, Some(5));
        write_settings(&mut config, &ModelSettings { seed: None, ..Default::default() });
        assert_eq!(read_settings(&config).seed, None);
        assert!(config.get("ai_seed").is_none());
    }

    #[test]
    fn generate_needs_a_model_name() {
        // A local endpoint, so the key plays no part in what this is testing.
        let local = |model: &str| GenerateDialog {
            settings: ModelSettings {
                endpoint: "http://localhost:11434/v1".into(),
                model: model.into(),
                seed: None,
            },
            ..GenerateDialog::default()
        };
        assert!(!local("").can_generate(), "there must be a model to ask for");
        assert!(!local("  ").can_generate(), "and whitespace is not a name");
        assert!(local("llama3.1:8b").can_generate());
        assert_eq!(
            local("").why_not_generate().as_deref(),
            Some("Enter the model to ask for")
        );
    }

    // A22 — the dialog will not start a request that must fail for want of a key.
    #[test]
    fn generate_is_unavailable_when_a_needed_key_is_absent() {
        let dialog = GenerateDialog::default(); // OpenAI endpoint, prefilled model.
        match dialog.key_status() {
            KeyStatus::Missing => {
                assert!(
                    !dialog.can_generate(),
                    "without a key the request can only fail, so do not offer it"
                );
                let why = dialog.why_not_generate().expect("a reason is given");
                assert!(why.contains(API_KEY_ENV), "the reason names the variable: {why}");
                assert!(dialog.key_status().line().contains(API_KEY_ENV));
            }
            KeyStatus::Present => {
                // This machine's environment happens to have a key; then Generate is
                // available and the dialog says where the key came from.
                assert!(dialog.can_generate());
                assert!(dialog.key_status().line().contains(API_KEY_ENV));
            }
            KeyStatus::NotNeeded => panic!("the default endpoint is OpenAI's and needs a key"),
        }
    }

    #[test]
    fn the_key_status_line_never_contains_a_key() {
        // Whatever the environment holds, the line the dialog shows is about the situation.
        for status in [KeyStatus::NotNeeded, KeyStatus::Present, KeyStatus::Missing] {
            let line = status.line();
            if let Some(key) = api_key_from_env() {
                assert!(!line.contains(&key), "the dialog showed the key: {line}");
            }
            assert!(!line.contains("Bearer"), "got: {line}");
        }
    }

    #[test]
    fn the_seed_is_read_from_its_text_box_and_a_bad_one_is_simply_absent() {
        let mut dialog = GenerateDialog::default();
        dialog.seed_text = "1234".into();
        assert_eq!(dialog.model_settings().seed, Some(1234));
        dialog.seed_text = "not a number".into();
        assert_eq!(dialog.model_settings().seed, None);
        dialog.seed_text = "".into();
        assert_eq!(dialog.model_settings().seed, None);
    }

    #[test]
    fn the_chamber_count_is_kept_to_the_range_the_slider_offers() {
        let mut dialog = GenerateDialog::default();
        dialog.chambers = 0;
        assert_eq!(dialog.params().chambers, CHAMBERS_MIN, "and not some other lower bound");
        dialog.chambers = 99;
        assert_eq!(dialog.params().chambers, CHAMBERS_MAX);
    }

    // The seed bug: restoring settings must fill the box the seed is actually read from.
    #[test]
    fn adopting_remembered_settings_fills_the_seed_box_too() {
        let mut dialog = GenerateDialog::default();
        dialog.adopt_settings(ModelSettings {
            endpoint: "http://localhost:11434/v1".into(),
            model: "llama3.1:8b".into(),
            seed: Some(4242),
        });
        assert_eq!(dialog.seed_text, "4242", "the box the seed is read from must be filled");
        assert_eq!(
            dialog.model_settings().seed,
            Some(4242),
            "otherwise the remembered seed is silently dropped and then erased"
        );
    }

    #[test]
    fn adopting_settings_with_no_seed_leaves_the_box_empty() {
        let mut dialog = GenerateDialog::default();
        dialog.seed_text = "999".into();
        dialog.adopt_settings(ModelSettings { seed: None, ..ModelSettings::default() });
        assert_eq!(dialog.seed_text, "", "a remembered absence is still an absence");
        assert_eq!(dialog.model_settings().seed, None);
    }

    /// Poll as the editor's frame loop would, until the worker is done.
    fn drain(run: &mut GenerateRun) -> Result<Generated, GenError> {
        let deadline = Instant::now() + std::time::Duration::from_secs(20);
        while run.poll() && Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        run.outcome.take().expect("the worker finished")
    }

    // A6 — the run is on a worker, and cancelling stops it with nothing delivered.
    #[test]
    fn a_run_happens_off_the_calling_thread_and_cancels_cleanly() {
        // A transcript that would otherwise succeed, so the failure below can only be the
        // cancellation: an earlier version used a client that returned nonsense, which
        // would have failed whether or not cancelling worked at all.
        let client = Box::new(ScriptedClient::always(
            r#"{"chambers":[{"name":"a","rect":[0,0,1000,1000],"role":"r"}],
                "spawn":[100,100],"exit":[200,200]}"#,
        ));
        let mut run = GenerateRun::start(client, GenerationParams::default());
        assert!(run.outcome.is_none(), "start returns before the worker has finished");

        run.cancel();
        let outcome = drain(&mut run);
        assert_eq!(
            outcome.expect_err("a cancelled run delivers no level"),
            GenError::Cancelled,
            "and says it was cancelled, not something else"
        );
    }

    #[test]
    fn a_worker_that_dies_is_not_reported_as_a_cancellation() {
        // A client that panics stands in for anything that kills the worker thread. The
        // channel then hangs up with no result, which must not be dressed up as the user
        // having pressed Cancel.
        struct Panicking;
        impl ModelClient for Panicking {
            fn complete(&mut self, _: &crate::ai_client::Request) -> Result<String, crate::ai_client::ModelError> {
                panic!("the worker died");
            }
        }
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {})); // The panic is expected; keep the output clean.
        let mut run = GenerateRun::start(Box::new(Panicking), GenerationParams::default());
        let outcome = drain(&mut run);
        std::panic::set_hook(previous);

        assert_eq!(
            outcome.expect_err("a dead worker delivers no level"),
            GenError::Worker,
            "a crash must not be reported as a cancellation"
        );
    }

    #[test]
    fn the_stage_line_names_the_stage_and_the_time() {
        let line = stage_line(
            &Progress::Chamber { index: 2, total: 5, name: "The Cistern".into() },
            75,
        );
        assert!(line.contains("Chamber 3 of 5"), "chambers are counted from one: {line}");
        assert!(line.contains("The Cistern"), "{line}");
        assert!(line.contains("1m 15s"), "a long wait reads as minutes: {line}");

        let proving = stage_line(&Progress::Proving, 8);
        assert!(proving.contains("Proving"), "{proving}");
        assert!(proving.contains("8s"), "{proving}");

        let repairing = stage_line(&Progress::Repairing, 0);
        assert!(repairing.contains("Repairing"), "{repairing}");
    }

    #[test]
    fn the_report_says_whether_the_level_is_the_models_own_work() {
        let untouched = Generated {
            level: LevelData::default(),
            repairs: vec![],
            skipped: vec![],
            route_moves: 12,
            route_secs: 34.0,
            chambers: vec!["a".into(), "b".into()],
        };
        let text = report(&untouched);
        assert!(text.contains("the model's own work"), "{text}");
        assert!(text.contains("nothing was changed"), "{text}");
        assert!(text.contains("2 chamber"), "{text}");
        assert!(text.contains("12 move"), "{text}");
        assert!(text.contains("34 seconds"), "the report says how long the run takes: {text}");

        let repaired = Generated {
            repairs: vec!["added a 260 x 40 ledge at (900, 1100) to carry the route on".into()],
            ..untouched
        };
        let text = report(&repaired);
        assert!(text.contains("1 ledge(s) were added"), "{text}");
        assert!(text.contains("(900, 1100)"), "the report names where: {text}");
    }

    #[test]
    fn the_report_names_anything_that_was_left_out() {
        let generated = Generated {
            level: LevelData::default(),
            repairs: vec![],
            skipped: vec!["in \"West Entry Hall\": entity 5 said only \"position\", \"type\"".into()],
            route_moves: 3,
            route_secs: 4.0,
            chambers: vec!["West Entry Hall".into()],
        };
        let text = report(&generated);
        assert!(text.contains("1 thing(s)"), "{text}");
        assert!(text.contains("West Entry Hall"), "and where: {text}");
        assert!(
            text.contains("the model's own work"),
            "what was delivered is still the model's: {text}"
        );
    }
}
