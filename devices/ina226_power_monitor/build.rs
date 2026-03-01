fn main() {
    if !std::path::Path::new("cfg.toml").exists() {
        panic!("You need to create a `cfg.toml` file. Use `cfg.toml.template` as a template.");
    }

    embuild::espidf::sysenv::output();
}
