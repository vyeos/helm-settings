//! Native GTK/libadwaita frontend for Helm.

#![forbid(unsafe_code)]

use adw::prelude::*;
use gtk::gio;
use helm_adapter_applications::{alacritty, theme, yazi};
use helm_adapter_bars::{quickshell, waybar};
use helm_adapter_hyprland::{HyprlandRuntime, ProcessRuntime};
use helm_core::{DiscoveryService, SystemProbe, XdgPaths, foundation_catalog};
use helm_transaction::Engine;

const APPLICATION_ID: &str = "io.github.vyeos.HelmSettings";

/// Launch the native Helm application.
pub fn run() {
    gio::resources_register_include!("helm-settings.gresource")
        .expect("embedded Helm resources must be valid");
    let application = adw::Application::builder()
        .application_id(APPLICATION_ID)
        .build();
    application.connect_startup(|_| install_css());
    application.connect_activate(build_window);
    application.run_with_args::<&str>(&[]);
}

fn install_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_resource("/io/github/vyeos/HelmSettings/style.css");
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn build_window(application: &adw::Application) {
    let builder = gtk::Builder::from_resource("/io/github/vyeos/HelmSettings/ui/window.ui");
    let window: adw::ApplicationWindow = builder.object("window").expect("window resource object");
    let stack: gtk::Stack = builder.object("stack").expect("stack resource object");
    let sidebar: gtk::StackSidebar = builder.object("sidebar").expect("sidebar resource object");
    let search: gtk::SearchEntry = builder.object("search").expect("search resource object");
    sidebar.set_stack(&stack);

    let report = DiscoveryService::new(SystemProbe).discover();
    let (overview, overview_content) = page(
        "Your desktop",
        "Helm audited the current session without changing configuration.",
    );
    let component_list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    for component in &report.components {
        let row = adw::ActionRow::builder()
            .title(glib::markup_escape_text(&component.display_name))
            .subtitle(glib::markup_escape_text(
                component.version.as_deref().unwrap_or_else(|| {
                    component
                        .notes
                        .first()
                        .map_or("Not detected", String::as_str)
                }),
            ))
            .build();
        let status = gtk::Label::new(Some(&format!("{:?}", component.availability)));
        status.add_css_class(
            if component.availability == helm_core::model::Availability::Available {
                "status-available"
            } else {
                "status-missing"
            },
        );
        row.add_suffix(&status);
        component_list.append(&row);
    }
    overview_content.append(&component_list);
    stack.add_titled(&overview, Some("overview"), "Overview");

    add_catalog_page(
        &stack,
        "appearance",
        "Appearance",
        "Themes and wallpaper",
        &["appearance", "wallpaper"],
    );
    add_desktop_page(&stack);
    add_applications_page(&stack);
    add_placeholder_page(
        &stack,
        "profiles",
        "Profiles",
        "Atomic desired-state profiles",
    );
    add_history_page(&stack);
    add_placeholder_page(
        &stack,
        "plugins",
        "Plugins",
        "Installed and sandboxed extensions",
    );
    add_diagnostics_page(&stack, &report);

    configure_search(&search, &stack);

    window.set_application(Some(application));
    window.present();
}

fn configure_search(search: &gtk::SearchEntry, stack: &gtk::Stack) {
    let (search_page, search_content) = page("Search", "Matching settings and modules");
    let search_results = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    search_content.append(&search_results);
    stack.add_named(&search_page, Some("search-results"));
    let stack_for_search = stack.clone();
    search.connect_search_changed(move |entry| {
        while let Some(child) = search_results.first_child() {
            search_results.remove(&child);
        }
        let query = entry.text().trim().to_lowercase();
        if query.is_empty() {
            stack_for_search.set_visible_child_name("overview");
            return;
        }
        for setting in foundation_catalog().into_iter().filter(|setting| {
            setting.label.to_lowercase().contains(&query)
                || setting.description.to_lowercase().contains(&query)
                || setting.id.0.contains(&query)
        }) {
            search_results.append(
                &adw::ActionRow::builder()
                    .title(glib::markup_escape_text(&setting.label))
                    .subtitle(glib::markup_escape_text(&setting.description))
                    .build(),
            );
        }
        stack_for_search.set_visible_child_name("search-results");
    });
}

fn page(title: &str, subtitle: &str) -> (gtk::ScrolledWindow, gtk::Box) {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(18)
        .margin_start(32)
        .margin_end(32)
        .margin_top(28)
        .margin_bottom(32)
        .build();
    let heading = gtk::Label::builder()
        .label(title)
        .xalign(0.0)
        .css_classes(["overview-title"])
        .build();
    let description = gtk::Label::builder()
        .label(subtitle)
        .xalign(0.0)
        .wrap(true)
        .build();
    content.append(&heading);
    content.append(&description);
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&content)
        .build();
    (scroll, content)
}

fn add_catalog_page(stack: &gtk::Stack, name: &str, title: &str, subtitle: &str, modules: &[&str]) {
    let (page, content) = page(title, subtitle);
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    for setting in foundation_catalog()
        .into_iter()
        .filter(|setting| modules.contains(&setting.module.as_str()))
    {
        list.append(
            &adw::ActionRow::builder()
                .title(glib::markup_escape_text(&setting.label))
                .subtitle(glib::markup_escape_text(&setting.description))
                .build(),
        );
    }
    if list.first_child().is_none() {
        list.append(
            &adw::ActionRow::builder()
                .title("Planned for this milestone stream")
                .subtitle("The adapter will appear here when its capability contract is active.")
                .build(),
        );
    }
    content.append(&list);
    stack.add_titled(&page, Some(name), title);
}

fn add_placeholder_page(stack: &gtk::Stack, name: &str, title: &str, subtitle: &str) {
    let (page, _) = page(title, subtitle);
    stack.add_titled(&page, Some(name), title);
}

fn add_desktop_page(stack: &gtk::Stack) {
    let (page, content) = page(
        "Desktop",
        "Hyprland general settings, displays and keybindings",
    );
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    match ProcessRuntime.descriptions() {
        Ok(catalog) => {
            for setting in catalog.settings {
                let subtitle = format!(
                    "{} · {:?}",
                    setting.definition.description, setting.effective
                );
                list.append(
                    &adw::ActionRow::builder()
                        .title(glib::markup_escape_text(&setting.definition.label))
                        .subtitle(glib::markup_escape_text(&subtitle))
                        .build(),
                );
            }
        }
        Err(error) => list.append(
            &adw::ActionRow::builder()
                .title("Hyprland settings unavailable")
                .subtitle(glib::markup_escape_text(&error.to_string()))
                .build(),
        ),
    }
    content.append(&list);
    stack.add_titled(&page, Some("desktop"), "Desktop");
}

fn add_applications_page(stack: &gtk::Stack) {
    let (page, content) = page(
        "Applications",
        "Shared themes and lossless application integrations",
    );
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    match XdgPaths::from_environment() {
        Ok(paths) => {
            let alacritty_status = format!("{:?}", alacritty::detect(&paths.config_home));
            list.append(
                &adw::ActionRow::builder()
                    .title("Alacritty")
                    .subtitle(glib::markup_escape_text(&alacritty_status))
                    .build(),
            );
            match yazi::discover_flavors(&paths.config_home) {
                Ok(flavors) => list.append(
                    &adw::ActionRow::builder()
                        .title("Yazi")
                        .subtitle(format!("{} installed flavors", flavors.len()))
                        .build(),
                ),
                Err(error) => list.append(
                    &adw::ActionRow::builder()
                        .title("Yazi")
                        .subtitle(glib::markup_escape_text(&error.to_string()))
                        .build(),
                ),
            }
            for palette in theme::builtins() {
                list.append(
                    &adw::ActionRow::builder()
                        .title(glib::markup_escape_text(&palette.name))
                        .subtitle("Built-in copy-on-write theme")
                        .build(),
                );
            }
            let waybar_status = format!("{:?}", waybar::detect(&paths.config_home));
            list.append(
                &adw::ActionRow::builder()
                    .title("Waybar")
                    .subtitle(glib::markup_escape_text(&waybar_status))
                    .build(),
            );
            let quickshell_status = format!("{:?}", quickshell::detect(&paths.config_home));
            list.append(
                &adw::ActionRow::builder()
                    .title("Quickshell")
                    .subtitle(glib::markup_escape_text(&quickshell_status))
                    .build(),
            );
        }
        Err(error) => list.append(
            &adw::ActionRow::builder()
                .title("Application paths unavailable")
                .subtitle(error)
                .build(),
        ),
    }
    content.append(&list);
    stack.add_titled(&page, Some("applications"), "Applications");
}

fn add_history_page(stack: &gtk::Stack) {
    let (page, content) = page("History", "Verified changes, rollback, and recovery");
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    if let Ok(paths) = XdgPaths::from_environment()
        && let Ok(engine) = Engine::open(paths.helm_state(), paths.writable_roots())
        && let Ok(history) = engine.history(100)
    {
        for entry in history {
            let subtitle = format!("{} · {:?}", entry.id, entry.state);
            list.append(
                &adw::ActionRow::builder()
                    .title(glib::markup_escape_text(&entry.summary))
                    .subtitle(glib::markup_escape_text(&subtitle))
                    .build(),
            );
        }
    }
    if list.first_child().is_none() {
        list.append(
            &adw::ActionRow::builder()
                .title("No changes yet")
                .subtitle("Every verified transaction will appear here.")
                .build(),
        );
    }
    content.append(&list);
    stack.add_titled(&page, Some("history"), "History");
}

fn add_diagnostics_page(stack: &gtk::Stack, report: &helm_core::model::EnvironmentReport) {
    let (page, content) = page("Diagnostics", "Local, redacted environment information");
    let label = gtk::Label::builder()
        .label(format!(
            "Session: {}\nSchema: {}\nDetected components: {}",
            report.session,
            report.schema_version,
            report.components.len()
        ))
        .xalign(0.0)
        .selectable(true)
        .build();
    content.append(&label);
    stack.add_titled(&page, Some("diagnostics"), "Diagnostics");
}
