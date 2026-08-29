use api::core::config::AppConfig;
use api::core::mail::Mailer;

#[test]
fn test_r22_mailer_noop_when_unconfigured() {
    let cfg = AppConfig::load_from_env().unwrap();
    let mailer = Mailer::new(&cfg);
    // If SMTP credentials are empty or omitted, mailer gracefully disables itself (R22)
    if cfg.smtp_user.is_none() {
        assert!(!mailer.is_enabled());
    }
}
