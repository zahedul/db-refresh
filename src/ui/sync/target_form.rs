use std::time::{Duration, Instant};

use dioxus::prelude::*;

use crate::database::connection_url::{ConnectionUrl, parse_connection_url};
use crate::database::credentials::{delete_password, save_password, unlock_password};
use crate::database::restore::test_connection;
use crate::database::target::{DatabaseTarget, DumpSource, upsert_target};
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
    source_profile: Signal<String>,
    source_bucket: Signal<String>,
    source_folder: Signal<String>,
    source_name_filter: Signal<String>,
    recreate_database: Signal<bool>,
    require_touch_id: Signal<bool>,
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
            recreate_database: (self.recreate_database)(),
            require_touch_id: (self.require_touch_id)(),
            dump_source: DumpSource {
                profile: (self.source_profile)().trim().to_string(),
                bucket: (self.source_bucket)().trim().to_lowercase(),
                folder_prefix: as_folder_prefix(&(self.source_folder)()),
                name_filter: (self.source_name_filter)().trim().to_string(),
            },
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
        source_profile: use_signal(|| draft.dump_source.profile.clone()),
        source_bucket: use_signal(|| draft.dump_source.bucket.clone()),
        source_folder: use_signal(|| draft.dump_source.folder_prefix.clone()),
        source_name_filter: use_signal(|| draft.dump_source.name_filter.clone()),
        recreate_database: use_signal(|| draft.recreate_database),
        require_touch_id: use_signal(|| draft.require_touch_id),
    };
    let mut should_save_password = use_signal(|| is_new || draft.is_password_saved);
    let mut is_password_visible = use_signal(|| false);
    let mut form_error = use_signal(|| None::<String>);
    let mut connection_check = use_signal(|| ConnectionCheck::NotRun);
    let mut is_confirming_delete = use_signal(|| false);

    let run_connection_test = {
        let draft = draft.clone();
        move |_| {
            let target = match fields.to_target(&draft) {
                Ok(target) => target,
                Err(error) => {
                    connection_check.set(ConnectionCheck::Failed(error.to_string()));
                    return;
                }
            };
            connection_check.set(ConnectionCheck::Checking);
            spawn(async move {
                let password = match unlock_password(&target, &(fields.password)()).await {
                    Ok(password) => password,
                    Err(error) => {
                        connection_check.set(ConnectionCheck::Failed(error.to_string()));
                        return;
                    }
                };
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
                if should_save_password() {
                    label { class: "switch",
                        input {
                            r#type: "checkbox",
                            checked: (fields.require_touch_id)(),
                            onchange: move |event| {
                                let mut require_touch_id = fields.require_touch_id;
                                require_touch_id.set(event.checked());
                            },
                        }
                        span { class: "switch-track", span { class: "switch-thumb" } }
                        span { class: "switch-text",
                            span { "Ask for Touch ID before using it" }
                            span { class: "field-hint", "Falls back to your Mac password when Touch ID isn't available." }
                        }
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

            FormSection { title: "Restore",
                label { class: "switch",
                    input {
                        r#type: "checkbox",
                        checked: (fields.recreate_database)(),
                        onchange: move |event| {
                            let mut recreate_database = fields.recreate_database;
                            recreate_database.set(event.checked());
                        },
                    }
                    span { class: "switch-track", span { class: "switch-thumb" } }
                    span { class: "switch-text",
                        span { "Start from an empty database" }
                        span { class: "field-hint",
                            "Deletes and recreates the database before each restore, so it becomes an exact copy of the dump. "
                            "Needs a user that owns the database and may create databases."
                        }
                    }
                }
            }

            FormSection { title: "One-click refresh",
                OneClickRefreshFields { fields }
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

/// Where "Refresh latest" finds this database's newest dump.
#[component]
fn OneClickRefreshFields(fields: TargetFields) -> Element {
    let settings = use_app_context().settings;

    let copy_current_source = move |_| {
        let current = settings.peek();
        let (mut profile, mut bucket, mut folder) = (
            fields.source_profile,
            fields.source_bucket,
            fields.source_folder,
        );
        profile.set(current.profile.clone().unwrap_or_default());
        bucket.set(current.bucket.clone());
        folder.set(current.folder_prefix.clone());
    };

    rsx! {
        div { class: "refresh-source-intro",
            span { class: "field-hint",
                "Where the newest dump for this database lives. Fill it in to get a ⟳ Refresh latest button."
            }
            button { class: "link-button", onclick: copy_current_source, "Use current S3 source" }
        }
        div { class: "form-row",
            FormField { label: "AWS profile", is_narrow: true,
                PlainTextInput { value: fields.source_profile, placeholder: "default" }
            }
            FormField { label: "Bucket",
                PlainTextInput { value: fields.source_bucket, placeholder: "my-backup-bucket" }
            }
        }
        div { class: "form-row",
            FormField { label: "Folder",
                PlainTextInput { value: fields.source_folder, placeholder: "backups/prod/" }
            }
            FormField { label: "Name contains", is_narrow: true,
                PlainTextInput { value: fields.source_name_filter, placeholder: "prod_" }
            }
        }
    }
}

/// `backups/prod` -> `backups/prod/`, so only that folder matches, not `backups/prod-old/`.
fn as_folder_prefix(typed_folder: &str) -> String {
    let folder = typed_folder.trim().trim_start_matches('/');
    if folder.is_empty() || folder.ends_with('/') {
        return folder.to_string();
    }
    format!("{folder}/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_prefix_gets_one_trailing_slash() {
        assert_eq!(as_folder_prefix(" backups/prod "), "backups/prod/");
        assert_eq!(as_folder_prefix("backups/prod/"), "backups/prod/");
        assert_eq!(as_folder_prefix("/backups"), "backups/");
        assert_eq!(as_folder_prefix(""), "");
    }
}
