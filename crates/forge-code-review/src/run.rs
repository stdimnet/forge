//! The AI calls: "Summarize PR", "AI Review" and "Explain this change". Each one checks the
//! policy first and sends only what [`crate::context`] lets through.

use forge_ai::{AiError, CompletionRequest, Message, Provider};
use forge_ai_editor::Policy;
use forge_forges::ChangedFile;
use serde::Serialize;

use crate::anchor::{anchor, Finding};
use crate::context::{allowed, plan, Budget, ContextPlan};
use crate::parse::{parse_findings, parse_summary, Summary};
use crate::prompt::{explain_prompt, review_prompt, summary_prompt, Prompt, RequestMeta};

#[derive(Debug, thiserror::Error)]
pub enum ReviewError {
    #[error("AI features are turned off for this project")]
    Disabled,
    #[error("{0} is excluded from AI by .aiignore")]
    Excluded(String),
    #[error("No changes can be sent: every changed file is excluded or generated")]
    NothingToSend,
    #[error("{0}")]
    NotFound(String),
    #[error(transparent)]
    Ai(#[from] AiError),
}

/// A provider with the model and policy of one run.
pub struct AiRun<'a> {
    pub provider: &'a dyn Provider,
    /// `None`: the provider's default model.
    pub model: Option<String>,
    pub policy: &'a Policy,
    pub budget: Budget,
}

impl AiRun<'_> {
    async fn ask(&self, p: Prompt) -> Result<String, ReviewError> {
        let reply = self
            .provider
            .complete(CompletionRequest {
                model: self.model.clone(),
                system: Some(p.system),
                messages: vec![Message::user(p.user)],
                max_tokens: Some(p.max_tokens),
                temperature: Some(0.2),
                stop: Vec::new(),
            })
            .await?;
        Ok(reply.text)
    }

    fn plan(&self, files: &[ChangedFile], budget: Budget) -> Result<ContextPlan, ReviewError> {
        if !self.policy.project_enabled {
            return Err(ReviewError::Disabled);
        }
        let p = plan(files, self.policy, budget);
        if p.chunks.is_empty() {
            return Err(ReviewError::NothingToSend);
        }
        Ok(p)
    }
}

/// The result of a run plus what was left out.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewOutcome<T> {
    pub result: T,
    pub excluded: Vec<String>,
    pub omitted: Vec<String>,
}

/// "Summarize PR": one request with the most important part of the diff.
pub async fn summarize(
    run: &AiRun<'_>,
    meta: &RequestMeta,
    files: &[ChangedFile],
) -> Result<ReviewOutcome<Summary>, ReviewError> {
    let p = run.plan(files, run.budget.single())?;
    let text = run.ask(summary_prompt(meta, &p)).await?;
    let mut summary = parse_summary(&text);
    // Never show a summary of a file the model was not given.
    summary.files.retain(|f| {
        p.files
            .iter()
            .any(|c| c.path == f.path && c.chunk.is_some())
    });
    Ok(ReviewOutcome {
        result: summary,
        excluded: p.excluded(),
        omitted: p.omitted(),
    })
}

/// "AI Review": one request per chunk; findings placed on the diff.
pub async fn review(
    run: &AiRun<'_>,
    meta: &RequestMeta,
    files: &[ChangedFile],
) -> Result<ReviewOutcome<Vec<Finding>>, ReviewError> {
    let p = run.plan(files, run.budget)?;
    let mut raw = Vec::new();
    for (i, chunk) in p.chunks.iter().enumerate() {
        let text = run.ask(review_prompt(meta, &p, chunk, i)).await?;
        // Findings only for files of this chunk (the model saw only their diff).
        raw.extend(parse_findings(&text).into_iter().filter(|f| {
            chunk
                .paths
                .iter()
                .any(|c| c == &f.path || c.ends_with(&format!("/{}", f.path)))
        }));
    }
    Ok(ReviewOutcome {
        result: anchor(raw, files),
        excluded: p.excluded(),
        omitted: p.omitted(),
    })
}

/// "Explain this change" of one hunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Explanation {
    /// Markdown.
    pub text: String,
    pub path: String,
    pub hunk: usize,
}

pub async fn explain(
    run: &AiRun<'_>,
    meta: &RequestMeta,
    files: &[ChangedFile],
    path: &str,
    hunk: usize,
) -> Result<Explanation, ReviewError> {
    if !run.policy.project_enabled {
        return Err(ReviewError::Disabled);
    }
    let file = files
        .iter()
        .find(|f| f.path == path)
        .ok_or_else(|| ReviewError::NotFound(format!("{path} is not part of this change")))?;
    if !allowed(run.policy, file) {
        return Err(ReviewError::Excluded(path.to_string()));
    }
    let prompt = explain_prompt(meta, file, hunk)
        .ok_or_else(|| ReviewError::NotFound(format!("{path} has no change #{}", hunk + 1)))?;
    Ok(Explanation {
        text: run.ask(prompt).await?.trim().to_string(),
        path: path.to_string(),
        hunk,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::tests::{file, hunk};
    use forge_ai::{AiResult, Completion, DeltaStream, ModelInfo};
    use forge_ai_editor::AiIgnore;
    use std::sync::Mutex;

    /// Answers with canned replies and records every request; no network.
    struct Fake {
        replies: Mutex<Vec<String>>,
        seen: Mutex<Vec<CompletionRequest>>,
    }

    impl Fake {
        fn new(replies: &[&str]) -> Self {
            Self {
                replies: Mutex::new(replies.iter().rev().map(|s| s.to_string()).collect()),
                seen: Mutex::new(Vec::new()),
            }
        }

        fn prompts(&self) -> Vec<String> {
            self.seen
                .lock()
                .unwrap()
                .iter()
                .map(|r| {
                    format!(
                        "{}\n{}",
                        r.system.clone().unwrap_or_default(),
                        r.messages[0].text_content()
                    )
                })
                .collect()
        }
    }

    #[async_trait::async_trait]
    impl Provider for Fake {
        fn name(&self) -> &str {
            "Fake"
        }
        fn default_model(&self) -> Option<&str> {
            Some("fake-1")
        }
        async fn list_models(&self) -> AiResult<Vec<ModelInfo>> {
            Ok(Vec::new())
        }
        async fn complete(&self, request: CompletionRequest) -> AiResult<Completion> {
            self.seen.lock().unwrap().push(request);
            Ok(Completion {
                text: self.replies.lock().unwrap().pop().unwrap_or_default(),
                model: "fake-1".into(),
                stop_reason: None,
                usage: None,
            })
        }
        async fn stream(&self, _: CompletionRequest) -> AiResult<DeltaStream> {
            Err(AiError::Cancelled)
        }
    }

    fn meta() -> RequestMeta {
        RequestMeta {
            request_name: "pull request".into(),
            title: "Roles".into(),
            ..RequestMeta::default()
        }
    }

    fn files() -> Vec<ChangedFile> {
        vec![
            file(
                "app/User.php",
                vec![hunk(10, &[(" ", "class User"), ("+", "public $role;")])],
            ),
            file(".env.example", vec![hunk(1, &[("+", "SECRET_TOKEN=abc")])]),
            file(
                "package-lock.json",
                vec![hunk(1, &[("+", "{\"lockfileVersion\": 3}")])],
            ),
        ]
    }

    fn policy() -> Policy {
        Policy {
            ignore: AiIgnore::parse(".env*"),
            ..Policy::default()
        }
    }

    #[tokio::test]
    async fn summary_leaves_excluded_files_on_the_machine() {
        let fake = Fake::new(&[
            r#"{"overview":"Adds a role.","files":[{"path":"app/User.php","summary":"Role property"},{"path":".env.example","summary":"invented"}],"testPlan":["Log in"]}"#,
        ]);
        let policy = policy();
        let run = AiRun {
            provider: &fake,
            model: None,
            policy: &policy,
            budget: Budget::default(),
        };
        let out = summarize(&run, &meta(), &files()).await.unwrap();
        assert_eq!(out.result.overview, "Adds a role.");
        assert_eq!(
            out.result.files.len(),
            1,
            "no summary for a file the model never saw"
        );
        assert_eq!(out.excluded, [".env.example"]);
        assert_eq!(out.omitted, ["package-lock.json"]);
        let prompts = fake.prompts();
        assert_eq!(prompts.len(), 1);
        assert!(prompts[0].contains("public $role;"));
        assert!(!prompts[0].contains("SECRET_TOKEN") && !prompts[0].contains(".env.example"));
        assert!(!prompts[0].contains("lockfileVersion"));
    }

    #[tokio::test]
    async fn review_places_findings_and_ignores_files_outside_the_chunk() {
        let fake = Fake::new(&[r#"Here you go:
{"findings":[{"path":"app/User.php","line":11,"severity":"warning","title":"Untyped","explanation":"Add a type.","suggestion":"public UserRole $role;"},
{"path":".env.example","line":1,"explanation":"Secret!"}]}"#]);
        let policy = policy();
        let run = AiRun {
            provider: &fake,
            model: Some("m".into()),
            policy: &policy,
            budget: Budget::default(),
        };
        let out = review(&run, &meta(), &files()).await.unwrap();
        assert_eq!(out.result.len(), 1);
        let f = &out.result[0];
        assert_eq!(f.position.as_ref().unwrap().new_line, Some(11));
        assert_eq!(f.suggestion.as_deref(), Some("public UserRole $role;"));
        assert_eq!(fake.seen.lock().unwrap()[0].model.as_deref(), Some("m"));
    }

    #[tokio::test]
    async fn nothing_is_sent_when_ai_is_off_or_the_file_is_ignored() {
        let fake = Fake::new(&["x"]);
        let off = Policy {
            project_enabled: false,
            ..Policy::default()
        };
        let run = AiRun {
            provider: &fake,
            model: None,
            policy: &off,
            budget: Budget::default(),
        };
        assert!(matches!(
            summarize(&run, &meta(), &files()).await,
            Err(ReviewError::Disabled)
        ));
        assert!(matches!(
            explain(&run, &meta(), &files(), "app/User.php", 0).await,
            Err(ReviewError::Disabled)
        ));
        let policy = policy();
        let run = AiRun {
            policy: &policy,
            ..run
        };
        assert!(matches!(
            explain(&run, &meta(), &files(), ".env.example", 0).await,
            Err(ReviewError::Excluded(_))
        ));
        let only_secret = vec![files().remove(1)];
        assert!(matches!(
            review(&run, &meta(), &only_secret).await,
            Err(ReviewError::NothingToSend)
        ));
        assert!(fake.prompts().is_empty());
    }

    #[tokio::test]
    async fn explains_one_hunk() {
        let fake = Fake::new(&["  It adds a role property.  "]);
        let policy = Policy::default();
        let run = AiRun {
            provider: &fake,
            model: None,
            policy: &policy,
            budget: Budget::default(),
        };
        let e = explain(&run, &meta(), &files(), "app/User.php", 0)
            .await
            .unwrap();
        assert_eq!(e.text, "It adds a role property.");
        assert!(fake.prompts()[0].contains("Explain this change in app/User.php"));
        assert!(explain(&run, &meta(), &files(), "app/User.php", 5)
            .await
            .is_err());
    }
}
