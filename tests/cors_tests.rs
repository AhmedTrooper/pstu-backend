use api::core::config::AppConfig;

#[test]
fn test_cors_config_default_and_env() {
    let cfg = AppConfig::load_from_env().unwrap();
    assert!(!cfg.cors_allowed_origins.is_empty());
}
