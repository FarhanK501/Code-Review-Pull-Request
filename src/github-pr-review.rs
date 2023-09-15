use dotenv::dotenv;
use flowsnet_platform_sdk::logger;
use github_flows::{
    get_octo, listen_to_event,
    octocrab::models::events::payload::{IssueCommentEventAction, PullRequestEventAction},
    octocrab::models::CommentId,
    EventPayload, GithubLogin
};
use http_req::{
    request::{Method, Request},
    uri::Uri,
};
use openai_flows::{
    chat::{ChatModel, ChatOptions},
    OpenAIFlows,
};
use std::env;

//  The soft character limit of the input context size
//   the max token size or word count for GPT4 is 8192
//   the max token size or word count for GPT35Turbo is 4096
static CHAR_SOFT_LIMIT: usize = 9000;
static MODEL: ChatModel = ChatModel::GPT35Turbo;
// static MODEL : ChatModel = ChatModel::GPT4;

#[no_mangle]
#[tokio::main(flavor = "current_thread")]
pub async fn run() -> anyhow::Result<()> {
    dotenv().ok();
    logger::init();
    log::debug!("Running function at github-pr-review/main");

    let owner = env::var("github_owner").unwrap_or("juntao".to_string());
    let repo = env::var("github_repo").unwrap_or("test".to_string());
    let trigger_phrase = env::var("trigger_phrase").unwrap_or("flows review".to_string());

    let events = vec!["pull_request", "issue_comment"];
    println!("MAGIC");
    listen_to_event(&GithubLogin::Default, &owner, &repo, events, |payload| {
        handler(
            &owner,
            &repo,
            &trigger_phrase,
            payload,
        )
    })
    .await;

    Ok(())
}

async fn handler(
    owner: &str,
    repo: &str,
    trigger_phrase: &str,
    payload: EventPayload,
) {
    // log::debug!("Received payload: {:?}", payload);
    let mut new_commit: bool = false;
    let (title, pull_number, _contributor) = match payload {
        EventPayload::PullRequestEvent(e) => {
            if e.action == PullRequestEventAction::Opened {
                log::debug!("Received payload: PR Opened");
            } else if e.action == PullRequestEventAction::Synchronize {
                new_commit = true;
                log::debug!("Received payload: PR Synced");
            } else {
                log::debug!("Not an Opened or Synchronize event for PR");
                return;
            }
            let p = e.pull_request;
            (
                p.title.unwrap_or("".to_string()),
                p.number,
                p.user.unwrap().login,
            )
        }
        EventPayload::IssueCommentEvent(e) => {
            if e.action == IssueCommentEventAction::Deleted {
                log::debug!("Deleted issue comment");
                return;
            }
            log::debug!("Other event for issue comment");

            let body = e.comment.body.unwrap_or_default();

            // if e.comment.performed_via_github_app.is_some() {
            //     return;
            // }
            // TODO: Makeshift but operational
            if body.starts_with("Hello, I am a [code review bot]") {
                log::info!("Ignore comment via bot");
                return;
            };

            if !body.to_lowercase().contains(&trigger_phrase.to_lowercase()) {
                log::info!("Ignore the comment without magic words");
                return;
            }

            (e.issue.title, e.issue.number, e.issue.user.login)
        }
        _ => return,
    };

    let chat_id = format!("PR#{}", pull_number);
    let system = &format!("You are a senior software engineer and developer. You are funny and sarcastic, you will review a source code file and its patch related to the subject of \"{}\".", title);
    let mut openai = OpenAIFlows::new();
    openai.set_retry_times(3);

    let octo = get_octo(&GithubLogin::Default);
    let issues = octo.issues(owner, repo);
    let mut comment_id: CommentId = 0u64.into();
    if new_commit {
        // Find the first "Hello, I am a [code review bot]" comment to update
        match issues.list_comments(pull_number).send().await {
            Ok(comments) => {
                for c in comments.items {
                    if c.body.unwrap_or_default().starts_with("Hello, I am a [code review bot]") {
                        comment_id = c.id;
                        break;
                    }
                }
            }
            Err(error) => {
                log::error!("Error getting comments: {}", error);
                return;
            }
        }
    } else {
        // PR OPEN or Trigger phrase: create a new comment
        match issues.create_comment(pull_number, "![CodeBot](data:image/jpeg;base64,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)\n\nHAnalyzing code is my cardio. You enjoy that coffee break!\n\n").await {
            Ok(comment) => {
                comment_id = comment.id;
            }
            Err(error) => {
                log::error!("Error posting comment: {}", error);
                return;
            }
        }
    }
    if comment_id == 0u64.into() { return; }

    let pulls = octo.pulls(owner, repo);
    let mut resp = String::new();
    resp.push_str("Ready to put on our code detective hats and uncover the mysteries!\n\n");
    resp.push_str("🕵️‍♂️ Let's Sherlock these code changes:\n\n");

    match pulls.list_files(pull_number).await {
        Ok(files) => {
            for f in files.items {
                let filename = &f.filename;
                if filename.ends_with(".md") || filename.ends_with(".js") || filename.ends_with(".css") || filename.ends_with(".html") || filename.ends_with(".htm") {
                    continue;
                }

                // The f.raw_url is a redirect. So, we need to construct our own here.
                let contents_url = f.contents_url.as_str();
                if contents_url.len() < 40 { continue; }
                let hash = &contents_url[(contents_url.len() - 40)..];
                let raw_url = format!(
                    "https://raw.githubusercontent.com/{owner}/{repo}/{}/{}", hash, filename
                );
                let file_uri = Uri::try_from(raw_url.as_str()).unwrap();
                let mut writer = Vec::new();
                match Request::new(&file_uri)
                    .method(Method::GET)
                    .header("Accept", "plain/text")
                    .header("User-Agent", "Flows Network Connector")
                    .send(&mut writer)
                    .map_err(|_e| {}) {
                        Err(_e) => {
                            log::error!("Cannot get file");
                            continue;
                        }
                        _ => {}
                }
                let file_as_text = String::from_utf8_lossy(&writer);
                let t_file_as_text = truncate(&file_as_text, CHAR_SOFT_LIMIT);

                resp.push_str("## [");
                resp.push_str(filename);
                resp.push_str("](");
                resp.push_str(f.blob_url.as_str());
                resp.push_str(")\n\n");

                log::debug!("Sending file to OpenAI: {}", filename);
                let co = ChatOptions {
                    model: MODEL,
                    restart: true,
                    system_prompt: Some(system),
                };
                let question = "Review the following source code and look for potential problems. The code might be truncated. So, do NOT comment on the completeness of the source code.\n\n".to_string() + t_file_as_text;
                match openai.chat_completion(&chat_id, &question, &co).await {
                    Ok(r) => {
                        resp.push_str(&r.choice);
                        resp.push_str("\n\n");
                        log::debug!("Received OpenAI resp for file: {}", filename);
                    }
                    Err(e) => {
                        log::error!("OpenAI returns error for file review for {}: {}", filename, e);
                    }
                }

                log::debug!("Sending patch to OpenAI: {}", filename);
                let co = ChatOptions {
                    model: MODEL,
                    restart: false,
                    system_prompt: Some(system),
                };
                let patch_as_text = f.patch.unwrap_or("".to_string());
                let t_patch_as_text = truncate(&patch_as_text, CHAR_SOFT_LIMIT);
                let question = "The following is a patch. Please summarize key changes.\n\n".to_string() + t_patch_as_text;
                match openai.chat_completion(&chat_id, &question, &co).await {
                    Ok(r) => {
                        resp.push_str(&r.choice);
                        resp.push_str("\n\n");
                        log::debug!("Received OpenAI resp for patch: {}", filename);
                    }
                    Err(e) => {
                        log::error!("OpenAI returns error for patch review for {}: {}", filename, e);
                    }
                }
            }
        },
        Err(_error) => {
            log::error!("Cannot get file list");
        }
    }

    resp.push_str("All done! 🚀 Time for some programming wisdom:\n\n");
    resp.push_str("> \"Clean code always looks like it was written by someone who cares.\" - Robert C. Martin (Uncle Bob)\n");
    resp.push_str("> \"It's not at all important to get it right the first time. It's vitally important to get it right the last time.\" - Andrew Hunt\n");

    // Send the entire response to GitHub PR
    // issues.create_comment(pull_number, resp).await.unwrap();
    match issues.update_comment(comment_id, resp).await {
        Err(error) => {
            log::error!("Error posting resp: {}", error);
        }
        _ => {}
    }
}

fn truncate(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        None => s,
        Some((idx, _)) => &s[..idx],
    }
}
