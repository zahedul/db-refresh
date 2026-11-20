use std::time::{Duration, Instant};

use dioxus::prelude::*;

use crate::database::connection_url::{ConnectionUrl, parse_connection_url};
use crate::database::credentials::{delete_password, save_password, unlock_password};
use crate::database::restore::test_connection;
use crate::database::target::{DatabaseTarget, DumpSource, RestoreMode, upsert_target};
use crate::error::AppError;
use crate::settings::Settings;
use crate::ui::app_context::{AppContext, use_app_context};
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
    restore_mode: Signal<RestoreMode>,
    should_require_touch_id: Signal<bool>,
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
            restore_mode: (self.restore_mode)(),
            should_require_touch_id: (self.should_require_touch_id)(),
            dump_source: DumpSource {
                profile: (self.source_profile)().trim().to_string(),
                bucket: (self.source_bucket)().trim().to_lowercase(),
                folder_prefix: normalize_folder_prefix(&(self.source_folder)()),
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
    let fields = use_target_fields(&draft);
    let should_save_password = use_signal(|| is_new || draft.is_password_saved);
    let mut form_error = use_signal(|| None::<String>);
    let connection_check = use_signal(|| ConnectionCheck::NotRun);
    let mut is_confirming_delete = use_signal(|| false);

    let run_connection_test = {
        let draft = draft.clone();
        move |_| test_target_connection(fields, &draft, connection_check)
    };
    let save_form = {
        let draft = draft.clone();
        move |_| match save_target_form(app, fields, &draft, should_save_password()) {
            Ok(()) => on_close.call(()),
            Err(error) => form_error.set(Some(error.to_string())),
        }
    };
    let delete_form_target = {
        let target_id = draft.id.clone();
        move |_| {
            if !is_confirming_delete() {
                is_confirming_delete.set(true);
                return;
            }
            match delete_target(app, &target_id) {
                Ok(()) => on_close.call(()),
                Err(error) => form_error.set(Some(error.to_string())),
            }
        }
    };

    let (title, subtitle) = describe_form(&draft, is_new);

    rsx! {
        Modal {
            title,
            subtitle,
            icon: rsx! { ServerIcon {} },
            on_close: move |_| on_close.call(()),

            if is_new {
                QuickFill { fields }
            }
            ConnectionFields { fields, should_focus_host: !is_new }
            AuthenticationFields { fields, should_save_password, is_password_saved: draft.is_password_saved }
            DisplayFields { fields }
            FormSection { title: "Before restoring",
                RestoreModeChoice { restore_mode: fields.restore_mode }
            }
            FormSection { title: "One-click refresh",
                OneClickRefreshFields { fields }
            }

            ConnectionCheckMessage { check: connection_check() }
            if let Some(message) = form_error() {
                p { class: "form-alert error", "{message}" }
            }
            TargetFormFooter {
                is_new,
                is_checking: connection_check() == ConnectionCheck::Checking,
                is_confirming_delete: is_confirming_delete(),
                on_delete: delete_form_target,
                on_test: run_connection_test,
                on_cancel: move |_| on_close.call(()),
                on_save: save_form,
            }
        }
    }
}

/// One signal per editable field, filled from `draft`.
fn use_target_fields(draft: &DatabaseTarget) -> TargetFields {
    TargetFields {
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
        restore_mode: use_signal(|| draft.restore_mode),
        should_require_touch_id: use_signal(|| draft.should_require_touch_id),
    }
}

fn describe_form(draft: &DatabaseTarget, is_new: bool) -> (String, String) {
    if is_new {
        return (
            "Add database".to_string(),
            "A PostgreSQL database that dumps can be restored into.".to_string(),
        );
    }
    (
        format!("Edit {}", draft.name),
        "Changes apply to the next sync.".to_string(),
    )
}

/// Connects with the form's values (asking for Touch ID if a saved password is used).
fn test_target_connection(
    fields: TargetFields,
    draft: &DatabaseTarget,
    mut connection_check: Signal<ConnectionCheck>,
) {
    let target = match fields.to_target(draft) {
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

fn save_target_form(
    app: AppContext,
    fields: TargetFields,
    draft: &DatabaseTarget,
    should_save_password: bool,
) -> Result<(), AppError> {
    let target = fields.to_target(draft)?;
    store_target(
        app.settings,
        target,
        &(fields.password)(),
        should_save_password,
    )
}

/// Removes the target, its saved password, and the selection if it pointed at it.
fn delete_target(app: AppContext, target_id: &str) -> Result<(), AppError> {
    delete_password(target_id)?;
    let (mut settings, mut selected_target_id) = (app.settings, app.selected_target_id);
    settings.with_mut(|settings| {
        settings
            .database_targets
            .retain(|target| target.id != target_id)
    });
    if selected_target_id.peek().as_deref() == Some(target_id) {
        selected_target_id.set(None);
    }
    Ok(())
}

#[component]
fn ConnectionFields(fields: TargetFields, should_focus_host: bool) -> Element {
    rsx! {
        FormSection { title: "Connection",
            div { class: "form-row",
                FormField { label: "Host",
                    PlainTextInput { value: fields.host, placeholder: "localhost", should_focus_on_mount: should_focus_host }
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
    }
}

#[component]
fn AuthenticationFields(
    fields: TargetFields,
    should_save_password: Signal<bool>,
    is_password_saved: bool,
) -> Element {
    let placeholder = if is_password_saved {
        "Saved in Keychain · type to replace"
    } else {
        "Leave empty to use ~/.pgpass"
    };
    let mut should_require_touch_id = fields.should_require_touch_id;

    rsx! {
        FormSection { title: "Authentication",
            FormField { label: "Password",
                PasswordInput { value: fields.password, placeholder }
            }
            ToggleSwitch {
                is_on: should_save_password(),
                on_toggle: move |is_on| should_save_password.set(is_on),
                label: "Remember password in macOS Keychain",
                hint: "Stored encrypted by macOS, never in the settings file.",
            }
            if should_save_password() {
                ToggleSwitch {
                    is_on: should_require_touch_id(),
                    on_toggle: move |is_on| should_require_touch_id.set(is_on),
                    label: "Ask for Touch ID before using it",
                    hint: "Falls back to your Mac password when Touch ID isn't available.",
                }
            }
        }
    }
}

#[component]
fn DisplayFields(fields: TargetFields) -> Element {
    rsx! {
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
    }
}

#[component]
fn TargetFormFooter(
    is_new: bool,
    is_checking: bool,
    is_confirming_delete: bool,
    on_delete: EventHandler<MouseEvent>,
    on_test: EventHandler<MouseEvent>,
    on_cancel: EventHandler<MouseEvent>,
    on_save: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div { class: "modal-footer",
            if !is_new {
                button { class: "danger-text", onclick: move |event| on_delete.call(event),
                    if is_confirming_delete { "Click again to delete" } else { "Delete" }
                }
            }
            div { class: "modal-footer-spacer" }
            button {
                class: "secondary with-icon",
                disabled: is_checking,
                onclick: move |event| on_test.call(event),
                if is_checking {
                    span { class: "spinner small" }
                    "Testing…"
                } else {
                    "Test connection"
                }
            }
            button { class: "secondary", onclick: move |event| on_cancel.call(event), "Cancel" }
            button { onclick: move |event| on_save.call(event),
                if is_new { "Add database" } else { "Save changes" }
            }
        }
    }
}

/// Password field with a Show/Hide button.
#[component]
fn PasswordInput(value: Signal<String>, placeholder: String) -> Element {
    let mut is_visible = use_signal(|| false);

    rsx! {
        div { class: "password-field",
            input {
                r#type: if is_visible() { "text" } else { "password" },
                placeholder,
                value: "{value}",
                autocomplete: "off",
                spellcheck: "false",
                oninput: move |event| value.set(event.value()),
            }
            button {
                class: "password-toggle",
                title: if is_visible() { "Hide password" } else { "Show password" },
                onclick: move |_| is_visible.set(!is_visible()),
                if is_visible() { "Hide" } else { "Show" }
            }
        }
    }
}

#[component]
fn ToggleSwitch(
    is_on: bool,
    on_toggle: EventHandler<bool>,
    label: String,
    hint: String,
) -> Element {
    rsx! {
        label { class: "switch",
            input {
                r#type: "checkbox",
                checked: is_on,
                onchange: move |event| on_toggle.call(event.checked()),
            }
            span { class: "switch-track", span { class: "switch-thumb" } }
            span { class: "switch-text",
                span { "{label}" }
                span { class: "field-hint", "{hint}" }
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
fn normalize_folder_prefix(typed_folder: &str) -> String {
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
        assert_eq!(normalize_folder_prefix(" backups/prod "), "backups/prod/");
        assert_eq!(normalize_folder_prefix("backups/prod/"), "backups/prod/");
        assert_eq!(normalize_folder_prefix("/backups"), "backups/");
        assert_eq!(normalize_folder_prefix(""), "");
    }
}

/// One card per restore mode; the selected one is highlighted.
#[component]
fn RestoreModeChoice(restore_mode: Signal<RestoreMode>) -> Element {
    rsx! {
        div { class: "mode-choice",
            for mode in RestoreMode::ALL {
                label {
                    key: "{mode.script_value()}",
                    class: "mode-option",
                    class: if restore_mode() == mode { "selected" },
                    input {
                        r#type: "radio",
                        name: "restore-mode",
                        checked: restore_mode() == mode,
                        onchange: move |_| restore_mode.set(mode),
                    }
                    span { class: "mode-option-text",
                        span { class: "mode-option-label", "{mode.label()}" }
                        span { class: "field-hint", "{mode.description()}" }
                    }
                }
            }
        }
    }
}
