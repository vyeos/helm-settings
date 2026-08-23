fn main() {
    glib_build_tools::compile_resources(
        &["resources"],
        "resources/io.github.vyeos.HelmSettings.gresource.xml",
        "helm-settings.gresource",
    );
}
