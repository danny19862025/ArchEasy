use gtk::{glib, prelude::*};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

const CLIENT: &str = include_str!("../scripts/ask-ai.py");

fn helper(action: String, request: Value, cancel: Arc<AtomicBool>, tx: mpsc::Sender<Value>) {
    let result = (|| -> Result<(), String> {
        let directory = glib::user_config_dir().join("archeasy/ask-ai");
        let mut child = Command::new("/usr/bin/python3")
            .args(["-c", CLIENT, &action])
            .arg(directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "Ask AI needs Python 3 to connect.".to_owned())?;
        let written = child
            .stdin
            .take()
            .ok_or("Could not open the private request channel.")
            .and_then(|mut input| {
                input
                    .write_all(request.to_string().as_bytes())
                    .map_err(|_| "Could not send the request.")
            });
        if let Err(error) = written {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error.into());
        }
        let output = child.stdout.take().ok_or("Could not read the reply.")?;
        let (lines_tx, lines_rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let Ok(line) = line else {
                    break;
                };
                if let Ok(event) = serde_json::from_str::<Value>(&line)
                    && lines_tx.send(event).is_err()
                {
                    break;
                }
            }
        });
        let deadline = Instant::now() + Duration::from_secs(240);
        let mut reported_error = false;
        loop {
            if cancel.load(Ordering::Relaxed) || Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Request stopped. The provider may already have counted or billed this request.".into());
            }
            match lines_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(event) => {
                    reported_error |= event["event"] == "error";
                    let _ = tx.send(event);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let status = child.wait().map_err(|_| "Could not finish the request.")?;
                    if !status.success() && !reported_error {
                        return Err("Ask AI stopped unexpectedly. Please try again.".into());
                    }
                    break;
                }
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        let _ = tx.send(json!({"event": "error", "text": error}));
    }
    let _ = tx.send(json!({"event": "finished"}));
}

struct Chat {
    provider: gtk::DropDown,
    api_key: gtk::PasswordEntry,
    api_row: gtk::Box,
    account_row: gtk::Box,
    connection_note: gtk::Label,
    account: gtk::DropDown,
    model: gtk::DropDown,
    login: gtk::Button,
    logout: gtk::Button,
    load: gtk::Button,
    send: gtk::Button,
    clear: gtk::Button,
    stop: gtk::Button,
    spinner: gtk::Spinner,
    consent: gtk::CheckButton,
    input: gtk::TextView,
    output: gtk::TextView,
    scroll: gtk::ScrolledWindow,
    status: gtk::Label,
    accounts: RefCell<Vec<Value>>,
    models: RefCell<Vec<Value>>,
    messages: RefCell<Vec<Value>>,
    pending: RefCell<Option<String>>,
    busy: Cell<bool>,
    updating: Cell<bool>,
    cancel: RefCell<Arc<AtomicBool>>,
}

impl Chat {
    fn provider_id(&self) -> &'static str {
        match self.provider.selected() {
            1 => "gemini",
            2 => "claude",
            _ => "openai",
        }
    }

    fn reset_models(&self) {
        self.models.borrow_mut().clear();
        self.model.set_model(Some(&gtk::StringList::new(&[
            "Load models to start chatting",
        ])));
    }
    fn client_id(&self) -> Option<String> {
        self.accounts
            .borrow()
            .get(self.account.selected().checked_sub(1)? as usize)?["client_id"]
            .as_str()
            .map(str::to_owned)
    }

    fn model_id(&self) -> Option<String> {
        self.models.borrow().get(self.model.selected() as usize)?["slug"]
            .as_str()
            .map(str::to_owned)
    }

    fn controls(&self) {
        let ready = !self.busy.get();
        let connected = if self.provider_id() != "openai" {
            !self.api_key.text().trim().is_empty()
        } else {
            self.account
                .selected()
                .checked_sub(1)
                .and_then(|i| self.accounts.borrow().get(i as usize).cloned())
                .is_some_and(|a| a["connected"] == true)
        };
        let openai = self.provider_id() == "openai";
        self.provider.set_sensitive(ready);
        self.api_key.set_editable(ready);
        self.api_row.set_visible(!openai);
        self.account_row.set_visible(openai);
        self.account.set_sensitive(ready);
        self.model
            .set_sensitive(ready && !self.models.borrow().is_empty());
        self.login.set_sensitive(ready);
        self.logout.set_sensitive(ready && connected);
        self.load.set_sensitive(ready && connected);
        self.clear.set_sensitive(ready);
        self.input.set_editable(ready);
        self.consent.set_sensitive(ready);
        self.send.set_sensitive(
            ready && connected && self.consent.is_active() && self.model_id().is_some(),
        );
        self.stop.set_visible(!ready);
        self.spinner.set_spinning(!ready);
    }

    fn append(&self, text: &str) {
        let buffer = self.output.buffer();
        buffer.insert(&mut buffer.end_iter(), text);
        let adjustment = self.scroll.vadjustment();
        glib::idle_add_local_once(move || {
            adjustment.set_value(adjustment.upper() - adjustment.page_size())
        });
    }

    fn new_chat(&self) {
        self.messages.borrow_mut().clear();
        self.pending.borrow_mut().take();
        self.output.buffer().set_text("");
        self.input.buffer().set_text("");
    }

    fn operation(self: &Rc<Self>, action: &str, mut request: Value) {
        if self.busy.replace(true) {
            return;
        }
        request["provider"] = json!(self.provider_id());
        if self.provider_id() != "openai" {
            request["api_key"] = json!(self.api_key.text().as_str());
        }
        self.status.set_text(match action {
            "login" => "Complete sign-in in your browser, then return here. You can cancel below.",
            "ask" => "Waiting for your AI provider…",
            "models" => "Loading available models…",
            "logout" => "Signing out…",
            _ => "Checking saved accounts…",
        });
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel.replace(cancel.clone());
        self.controls();
        let (tx, rx) = mpsc::channel();
        let action = action.to_owned();
        std::thread::spawn(move || helper(action, request, cancel, tx));
        let this = self.clone();
        glib::timeout_add_local(Duration::from_millis(50), move || {
            while let Ok(event) = rx.try_recv() {
                match event["event"].as_str().unwrap_or("") {
                    "authorize" => {
                        if let Some(url) = event["url"].as_str()
                            && url.starts_with("https://auth.openai.com/api/accounts/authorize?")
                            && gtk::gio::AppInfo::launch_default_for_uri(
                                url,
                                None::<&gtk::gio::AppLaunchContext>,
                            )
                            .is_err()
                        {
                            this.cancel.borrow().store(true, Ordering::Relaxed);
                            this.status.set_text("Could not open your browser. Check your default browser and try again.");
                        }
                    }
                    "state" => {
                        let accounts = event["accounts"].as_array().cloned().unwrap_or_default();
                        let mut labels = vec!["Add a ChatGPT account…".to_owned()];
                        labels.extend(accounts.iter().map(|a| {
                            let id = a["client_id"].as_str().unwrap_or("");
                            let suffix = &id[id.len().saturating_sub(8)..];
                            format!(
                                "{} · {}{}",
                                a["label"].as_str().unwrap_or("ChatGPT"),
                                suffix,
                                if a["connected"] == true {
                                    ""
                                } else {
                                    " · signed out"
                                }
                            )
                        }));
                        let active = accounts
                            .iter()
                            .position(|a| a["client_id"] == event["active"])
                            .map(|i| i as u32 + 1)
                            .unwrap_or(0);
                        this.updating.set(true);
                        this.account.set_model(Some(&gtk::StringList::new(
                            &labels.iter().map(String::as_str).collect::<Vec<_>>(),
                        )));
                        this.account.set_selected(active);
                        this.accounts.replace(accounts);
                        this.updating.set(false);
                        this.status.set_text(if active == 0 { "Continue with ChatGPT to get started." }
                            else { "Account selected. Load models to start chatting, or continue with ChatGPT to reconnect." });
                    }
                    "models" => {
                        let models = event["models"].as_array().cloned().unwrap_or_default();
                        let names: Vec<_> =
                            models.iter().filter_map(|m| m["name"].as_str()).collect();
                        this.model.set_model(Some(&gtk::StringList::new(&names)));
                        this.model.set_selected(0);
                        let empty = models.is_empty();
                        this.models.replace(models);
                        this.status.set_text(if empty { "No models are available. Check your account permissions and provider access." }
                            else { "Ready. Select a model and send your question." });
                    }
                    "delta" => this.append(event["text"].as_str().unwrap_or("")),
                    "answer" => {
                        if let Some(question) = this.pending.borrow_mut().take() {
                            let mut messages = this.messages.borrow_mut();
                            messages.push(json!({"role":"user", "content":question}));
                            messages.push(json!({"role":"assistant", "content":event["text"]}));
                        }
                        this.input.buffer().set_text("");
                        this.append("\n\n");
                        this.status.set_text("Reply complete. Review any suggested commands before running them yourself.");
                    }
                    "error" => {
                        if this.pending.borrow_mut().take().is_some() {
                            this.append("\n[Reply not completed. Your message remains in the input box for retry.]\n\n");
                        }
                        this.status
                            .set_text(event["text"].as_str().unwrap_or("Request failed."));
                    }
                    "notice" => this.status.set_text(event["text"].as_str().unwrap_or("")),
                    "finished" => {
                        this.busy.set(false);
                        this.controls();
                        return glib::ControlFlow::Break;
                    }
                    _ => {}
                }
            }
            glib::ControlFlow::Continue
        });
    }
}

pub fn page() -> gtk::Box {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
    body.set_widget_name("ask-ai-page");
    let title = gtk::Label::new(Some("Ask AI"));
    title.add_css_class("title-2");
    title.set_xalign(0.0);
    body.append(&title);
    let note = gtk::Label::new(Some(
        "Ask about Arch Linux packages, gaming setup, or an error. Replies can suggest commands; ArchEasy never runs them from chat.",
    ));
    note.set_wrap(true);
    note.set_xalign(0.0);
    body.append(&note);
    let provider = gtk::DropDown::from_strings(&["ChatGPT", "Gemini", "Claude"]);
    provider.set_widget_name("ask-ai-provider");
    provider.set_hexpand(true);
    let providers = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    providers.append(&gtk::Label::new(Some("Provider")));
    providers.append(&provider);
    body.append(&providers);
    let connection_note = gtk::Label::new(Some(
        "ChatGPT uses your eligible ChatGPT plan. Continue with ChatGPT to connect.",
    ));
    connection_note.set_wrap(true);
    connection_note.set_xalign(0.0);
    body.append(&connection_note);
    let api_key = gtk::PasswordEntry::new();
    api_key.set_widget_name("ask-ai-api-key");
    api_key.set_placeholder_text(Some("API key — kept only while this provider is selected"));
    api_key.set_hexpand(true);
    api_key.set_show_peek_icon(true);
    let key_link =
        gtk::LinkButton::with_label("https://aistudio.google.com/apikey", "Get API key ↗");
    let api_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    api_row.append(&api_key);
    api_row.append(&key_link);
    body.append(&api_row);
    let account = gtk::DropDown::from_strings(&["Add a ChatGPT account…"]);
    account.set_hexpand(true);
    account.set_widget_name("ask-ai-account");
    let login = gtk::Button::with_label("Continue with ChatGPT");
    login.set_widget_name("ask-ai-login");
    let logout = gtk::Button::with_label("Sign out");
    let accounts = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    accounts.append(&account);
    accounts.append(&login);
    accounts.append(&logout);
    body.append(&accounts);
    let model = gtk::DropDown::from_strings(&["Sign in and load models"]);
    model.set_widget_name("ask-ai-model");
    model.set_hexpand(true);
    let load = gtk::Button::with_label("Load models");
    let clear = gtk::Button::with_label("New chat");
    let model_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    model_row.append(&model);
    model_row.append(&load);
    model_row.append(&clear);
    body.append(&model_row);
    let output = gtk::TextView::new();
    output.set_widget_name("ask-ai-conversation");
    output.set_editable(false);
    output.set_wrap_mode(gtk::WrapMode::WordChar);
    output.set_left_margin(12);
    output.set_right_margin(12);
    output.set_top_margin(12);
    output.set_bottom_margin(12);
    let scroll = gtk::ScrolledWindow::builder()
        .child(&output)
        .vexpand(true)
        .min_content_height(160)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    scroll.add_css_class("card");
    body.append(&scroll);
    let input = gtk::TextView::new();
    input.set_widget_name("ask-ai-message");
    input.set_wrap_mode(gtk::WrapMode::WordChar);
    input.set_left_margin(10);
    input.set_right_margin(10);
    input.set_top_margin(8);
    input.set_bottom_margin(8);
    let entry_scroll = gtk::ScrolledWindow::builder()
        .child(&input)
        .min_content_height(72)
        .max_content_height(120)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    entry_scroll.add_css_class("card");
    body.append(&entry_scroll);
    let consent =
        gtk::CheckButton::with_label("Send this conversation to OpenAI using my ChatGPT plan");
    consent.set_widget_name("ask-ai-consent");
    body.append(&consent);
    let privacy = gtk::Label::new(Some(
        "Only what you type here is sent. No files or system details are collected. Chat history stays in memory; New chat clears it. The selected provider’s usage limits and billing apply.",
    ));
    privacy.set_wrap(true);
    privacy.set_xalign(0.0);
    privacy.add_css_class("dim-label");
    body.append(&privacy);
    let send = gtk::Button::with_label("Send");
    send.set_widget_name("ask-ai-send");
    send.add_css_class("suggested-action");
    let stop = gtk::Button::with_label("Cancel");
    let spinner = gtk::Spinner::new();
    let status = gtk::Label::new(Some("Continue with ChatGPT to get started."));
    status.set_widget_name("ask-ai-status");
    status.set_wrap(true);
    status.set_hexpand(true);
    status.set_xalign(0.0);
    let bottom = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    bottom.append(&status);
    bottom.append(&spinner);
    bottom.append(&stop);
    bottom.append(&send);
    body.append(&bottom);
    let chat = Rc::new(Chat {
        provider,
        api_key,
        api_row,
        account_row: accounts,
        connection_note,
        account,
        model,
        login,
        logout,
        load,
        send,
        clear,
        stop,
        spinner,
        consent,
        input,
        output,
        scroll,
        status,
        accounts: RefCell::new(vec![]),
        models: RefCell::new(vec![]),
        messages: RefCell::new(vec![]),
        pending: RefCell::new(None),
        busy: Cell::new(false),
        updating: Cell::new(false),
        cancel: RefCell::new(Arc::new(AtomicBool::new(false))),
    });
    let c = chat.clone();
    chat.provider.connect_selected_notify(move |_| {
        c.new_chat();
        c.consent.set_active(false);
        c.api_key.set_text("");
        c.reset_models();
        let (notice, consent_text, url) = match c.provider_id() {
            "gemini" => ("Gemini uses a Google AI Studio API key, not a Gemini app subscription. API quotas, billing and data terms apply. Keys are not saved to disk.", "Send this conversation to Google using my Gemini API key", "https://aistudio.google.com/apikey"),
            "claude" => ("Claude uses an Anthropic API key, not a Claude Pro/Max subscription. API usage is billed separately. Keys are not saved to disk.", "Send this conversation to Anthropic using my Claude API key", "https://platform.claude.com/settings/keys"),
            _ => ("ChatGPT uses your eligible ChatGPT plan. Continue with ChatGPT to connect.", "Send this conversation to OpenAI using my ChatGPT plan", "https://aistudio.google.com/apikey"),
        };
        c.connection_note.set_text(notice);
        c.consent.set_label(Some(consent_text));
        key_link.set_uri(url);
        c.status.set_text(if c.provider_id() == "openai" { "Continue with ChatGPT or load models for a saved account." } else { "Enter your API key privately here, then click Load models. API usage may incur charges." });
        c.controls();
    });
    let c = chat.clone();
    chat.api_key.connect_changed(move |_| {
        // A changed key must not inherit a previous key's model catalog or chat.
        c.new_chat();
        c.reset_models();
        c.consent.set_active(false);
        c.controls();
    });
    let c = chat.clone();
    chat.login.connect_clicked(move |_| {
        c.models.borrow_mut().clear();
        c.new_chat();
        c.operation("login", json!({"client_id": c.client_id()}));
    });
    let c = chat.clone();
    chat.load
        .connect_clicked(move |_| c.operation("models", json!({"client_id": c.client_id()})));
    let c = chat.clone();
    chat.logout.connect_clicked(move |_| {
        c.new_chat();
        c.models.borrow_mut().clear();
        c.operation("logout", json!({"client_id": c.client_id()}));
    });
    let c = chat.clone();
    chat.account.connect_selected_notify(move |_| {
        if !c.updating.get() {
            c.new_chat();
            c.models.borrow_mut().clear();
            c.model.set_model(Some(&gtk::StringList::new(&[
                "Load models for this account",
            ])));
            c.status
                .set_text("Account selected. Continue with ChatGPT or load its available models.");
            c.controls();
        }
    });
    let c = chat.clone();
    chat.model.connect_selected_notify(move |_| c.controls());
    let c = chat.clone();
    chat.consent.connect_toggled(move |_| c.controls());
    let c = chat.clone();
    chat.clear.connect_clicked(move |_| c.new_chat());
    let c = chat.clone();
    chat.stop
        .connect_clicked(move |_| c.cancel.borrow().store(true, Ordering::Relaxed));
    let c = chat.clone();
    chat.send.connect_clicked(move |_| {
        if c.busy.get() || !c.consent.is_active() {
            return;
        }
        let buffer = c.input.buffer();
        let question = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .trim()
            .to_owned();
        if question.is_empty() {
            c.status.set_text("Type a question first.");
            return;
        }
        let Some(model) = c.model_id() else {
            return;
        };
        let mut messages = c.messages.borrow().clone();
        messages.push(json!({"role":"user", "content":question}));
        c.append(&format!("You\n{question}\n\nAssistant\n"));
        c.pending.replace(Some(question));
        c.operation(
            "ask",
            json!({"client_id":c.client_id(), "model":model, "messages":messages}),
        );
    });
    chat.operation("status", json!({}));
    body
}
