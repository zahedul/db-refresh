use std::time::{Duration, Instant};

use dioxus::prelude::*;

use crate::database::connection_url::{ConnectionUrl, parse_connection_url};
use crate::database::credentials::{delete_password, password_for, save_password};
use crate::database::restore::test_connection;
use crate::database::target::{DatabaseTarget, upsert_target};
use crate::error::AppError;
use crate::settings::Settings;
use crate::ui::app_context::use_app_context;
use crate::ui::icons::ServerIcon;
use crate::ui::modal::Modal;
use crate::ui::plain_text_input::PlainTextInput;

#[derive(Clone, Debug, PartialEq)]
enum ConnectionCheck {
    NotRun,
    Checking,
    Succeeded { took: Duration },
    Failed(String),
}

/// The editable fields of the form; `Copy` because every field is a `Signal`.
#[derive(Clone, Copy, PartialEq)]
struct TargetFields {
    name: Signal<String>,
    host: Signal<String>,
    port: Signal<String>,
    user: Signal<String>,
    database: Signal<String>,
    dump_group: Signal<String>,
    password: Signal<String>,
}

impl TargetFields {
    fn to_target(self, draft: &DatabaseTarget) -> Result<DatabaseTarget, AppError> {
        let port = (self.port)().trim().parse::<u16>().map_err(|_| {
            AppError::InvalidTarget("Port must be a number between 1 and 65535.".to_string())
        })?;
        let mut target = DatabaseTarget {
            name: (self.name)(),
            host: (self.host)(),
            port,
            user: (self.user)(),
            database: (self.database)(),
            dump_group: (self.dump_group)(),
            ..draft.clone()
        }
        .trimmed();
        // The name is optional: most people name a target after its database.
        if target.name.is_empty() {
            target.name = target.database.clone();
        }
        target.validate()?;
        Ok(target)
    }

    /// Copies whatever the pasted URL contains; fields it lacks keep their value.
    fn fill_from(mut self, url: ConnectionUrl) {
        self.host.set(url.host);
        if let Some(port) = url.port {
            self.port.set(port.to_string());
        }
        if let Some(user) = url.user {
            self.user.set(user);
        }
        if let Some(database) = url.database {
            self.database.set(database);
        }
        if let Some(password) = url.password {
            self.password.set(password);
        }
    }

    /// `postgres@localhost:5432/app_db`, with placeholders for empty fields.
    fn preview(self) -> String {
        let or_placeholder = |value: String, placeholder: &str| {
            let value = value.trim().to_string();
            if value.is_empty() {
                placeholder.to_string()
            } else {
                value
            }
        };
        format!(
            "{}@{}:{}/{}",
            or_placeholder((self.user)(), "user"),
            or_placeholder((self.host)(), "host"),
            or_placeholder((self.port)(), "port"),
            or_placeholder((self.database)(), "database"),
        )
    }
}

/// Add or edit a database target, including its Keychain password.
#[component]
pub fn TargetFormDialog(
    draft: DatabaseTarget,
    is_new: bool,
    on_close: EventHandler<()>,
) -> Element {
    let app = use_app_context();
    let fields = TargetFields {
        name: use_signal(|| draft.name.clone()),
        host: use_signal(|| draft.host.clone()),
        port: use_signal(|| draft.port.to_string()),
        user: use_signal(|| draft.user.clone()),
        database: use_signal(|| draft.database.clone()),
        dump_group: use_signal(|| draft.dump_group.clone()),
        password: use_signal(String::new),
    };
    let mut should_save_password = use_signal(|| is_new || draft.is_password_saved);
    let mut is_password_visible = use_signal(|| false);
    let mut form_error = use_signal(|| None::<String>);
    let mut connection_check = use_signal(|| ConnectionCheck::NotRun);
    let mut is_confirming_delete = use_signal(|| false);

    let run_connection_test = {
        let draft = draft.clone();
        move |_| {
            let checked_target = fields
                .to_target(&draft)
                .and_then(|target| Ok((password_for(&target, &(fields.password)())?, target)));
            let (password, target) = match checked_target {
                Ok(values) => values,
                Err(error) => {
                    connection_check.set(ConnectionCheck::Failed(error.to_string()));
                    return;
                }
            };
            connection_check.set(ConnectionCheck::Checking);
            spawn(async move {
                let started_at = Instant::now();
                let next_check = match test_connection(&target, password.as_deref()).await {
                    Ok(()) => ConnectionCheck::Succeeded {
                        took: started_at.elapsed(),
                    },
                    Err(error) => ConnectionCheck::Failed(error.to_string()),
                };
                connection_check.set(next_check);
            });
        }
    };

    let save_target = {
        let draft = draft.clone();
        move |_| {
            let saved = fields.to_target(&draft).and_then(|target| {
                store_target(
                    app.settings,
                    target,
                    &(fields.password)(),
                    should_save_password(),
                )
            });
            match saved {
                Ok(()) => on_close.call(()),
                Err(error) => form_error.set(Some(error.to_string())),
            }
        }
    };

    let delete_target = {
        let target_id = draft.id.clone();
        move |_| {
            if !is_confirming_delete() {
                is_confirming_delete.set(true);
                return;
            }
            if let Err(error) = delete_password(&target_id) {
                form_error.set(Some(error.to_string()));
                return;
            }
            let (mut settings, mut selected_target_id) = (app.settings, app.selected_target_id);
            settings.with_mut(|settings| {
                settings
                    .database_targets
                    .retain(|target| target.id != target_id)
            });
            if selected_target_id.peek().as_deref() == Some(target_id.as_str()) {
                selected_target_id.set(None);
            }
            on_close.call(());
        }
    };

    let (title, subtitle) = if is_new {
        (
            "Add database".to_string(),
            "A PostgreSQL database that dumps can be restored into.".to_string(),
        )
    } else {
        (
            format!("Edit {}", draft.name),
            "Changes apply to the next sync.".to_string(),
        )
    };
    let password_placeholder = if draft.is_password_saved {
        "Saved in Keychain · type to replace"
    } else {
        "Leave empty to use ~/.pgpass"
    };
    let is_checking = connection_check() == ConnectionCheck::Checking;

    rsx! {
        Modal {
            title,
            subtitle,
            icon: rsx! { ServerIcon {} },
            on_close: move |_| on_close.call(()),

            if is_new {
                QuickFill { fields }
            }

            FormSection { title: "Connection",
                div { class: "form-row",
                    FormField { label: "Host",
                        PlainTextInput { value: fields.host, placeholder: "localhost", focus_on_mount: !is_new }
                    }
                    FormField { label: "Port", is_narrow: true,
                        PlainTextInput { value: fields.port, placeholder: "5432" }
                    }
                }
                div { class: "form-row",
                    FormField { label: "Database",
                        PlainTextInput { value: fields.database, placeholder: "app_db" }
                    }
                    FormField { label: "User",
                        PlainTextInput { value: fields.user, placeholder: "postgres" }
                    }
                }
                p { class: "connection-preview mono", "{fields.preview()}" }
            }

            FormSection { title: "Authentication",
                FormField { label: "Password",
                    div { class: "password-field",
                        input {
                            r#type: if is_password_visible() { "text" } else { "password" },
                            placeholder: password_placeholder,
                            value: "{fields.password}",
                            autocomplete: "off",
                            spellcheck: "false",
                            oninput: move |event| {
                                let mut password = fields.password;
                                password.set(event.value());
                            },
                        }
                        button {
                            class: "password-toggle",
                            title: if is_password_visible() { "Hide password" } else { "Show password" },
                            onclick: move |_| is_password_visible.set(!is_password_visible()),
                            if is_password_visible() { "Hide" } else { "Show" }
                        }
                    }
                }
                label { class: "switch",
                    input {
                        r#type: "checkbox",
                        checked: should_save_password(),
                        onchange: move |event| should_save_password.set(event.checked()),
                    }
                    span { class: "switch-track", span { class: "switch-thumb" } }
                    span { class: "switch-text",
                        span { "Remember password in macOS Keychain" }
                        span { class: "field-hint", "Stored encrypted by macOS, never in the settings file." }
                    }
                }
            }

            FormSection { title: "Display",
                div { class: "form-row",
                    FormField { label: "Name", hint: "Defaults to the database name.",
                        PlainTextInput { value: fields.name, placeholder: "Staging (local)" }
                    }
                    FormField { label: "Dump prefix", hint: "prod_… dumps pick this database.",
                        PlainTextInput { value: fields.dump_group, placeholder: "prod" }
                    }
                }
            }

            ConnectionCheckMessage { check: connection_check() }
            if let Some(message) = form_error() {
                p { class: "form-alert error", "{message}" }
            }

            div { class: "modal-footer",
                if !is_new {
                    button { class: "danger-text", onclick: delete_target,
                        if is_confirming_delete() { "Click again to delete" } else { "Delete" }
                    }
                }
                div { class: "modal-footer-spacer" }
                button {
                    class: "secondary with-icon",
                    disabled: is_checking,
                    onclick: run_connection_test,
                    if is_checking {
                        span { class: "spinner small" }
                        "Testing…"
                    } else {
                        "Test connection"
                    }
                }
                button { class: "secondary", onclick: move |_| on_close.call(()), "Cancel" }
                button { onclick: save_target,
                    if is_new { "Add database" } else { "Save changes" }
                }
            }
        }
    }
}

/// Paste a `postgres://…` URL to fill the connection fields in one go.
#[component]
fn QuickFill(fields: TargetFields) -> Element {
    let mut url_text = use_signal(String::new);
    let mut was_filled = use_signal(|| false);

    let on_url_input = move |event: FormEvent| {
        let text = event.value();
        let Some(parsed) = parse_connection_url(&text) else {
            url_text.set(text);
            was_filled.set(false);
            return;
        };
        fields.fill_from(parsed);
        // Clear it: the URL may contain the password in plain text.
        url_text.set(String::new());
        was_filled.set(true);
    };

    rsx! {
        div { class: "quick-fill",
            span { class: "field-label", "Quick fill" }
            input {
                r#type: "text",
                class: "mono",
                placeholder: "Paste a URL: postgres://user:password@localhost:5432/app_db",
                value: "{url_text}",
                spellcheck: "false",
                autocomplete: "off",
                "autocapitalize": "off",
                "autocorrect": "off",
                oninput: on_url_input,
            }
            if was_filled() {
                span { class: "field-hint success", "✓ Filled from the URL. Check the fields below." }
            }
        }
    }
}

/// Saves or removes the Keychain password as chosen, then stores the target in settings.
fn store_target(
    mut settings: Signal<Settings>,
    mut target: DatabaseTarget,
    typed_password: &str,
    should_save_password: bool,
) -> Result<(), AppError> {
    if should_save_password && !typed_password.is_empty() {
        save_password(&target.id, typed_password)?;
        target.is_password_saved = true;
    }
    if !should_save_password {
        if target.is_password_saved {
            delete_password(&target.id)?;
        }
        target.is_password_saved = false;
    }
    // Checked but left empty: keep whatever is already saved.
    settings.with_mut(|settings| upsert_target(&mut settings.database_targets, target));
    Ok(())
}

#[component]
fn FormSection(title: String, children: Element) -> Element {
    rsx! {
        section { class: "form-section",
            p { class: "form-section-title", "{title}" }
            div { class: "form-grid", {children} }
        }
    }
}

#[component]
fn FormField(
    label: String,
    #[props(default)] hint: Option<String>,
    #[props(default)] is_narrow: bool,
    children: Element,
) -> Element {
    rsx! {
        div { class: "form-field", class: if is_narrow { "narrow" },
            span { class: "field-label", "{label}" }
            {children}
            if let Some(hint) = hint {
                span { class: "field-hint", "{hint}" }
            }
        }
    }
}

#[component]
fn ConnectionCheckMessage(check: ConnectionCheck) -> Element {
    match check {
        ConnectionCheck::NotRun | ConnectionCheck::Checking => rsx! {},
        ConnectionCheck::Succeeded { took } => rsx! {
            p { class: "form-alert success", "✓ Connected in {took.as_millis()} ms" }
        },
        ConnectionCheck::Failed(message) => rsx! {
            p { class: "form-alert error", "✗ {message}" }
        },
    }
}
