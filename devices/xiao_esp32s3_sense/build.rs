fn main() {
    // Check if the `cfg.toml` file exists and has been filled out.
    if !std::path::Path::new("cfg.toml").exists() {
        panic!("You need to create a `cfg.toml` file with your Wi-Fi credentials! Use `cfg.toml.example` as a template.");
    }
    // Make App_config available as a system environment variable.
    embuild::espidf::sysenv::output();
}
