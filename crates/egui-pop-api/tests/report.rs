//! Report a problem: `help.report` is there only when the application
//! says where issues go, in the Help menu and the palette; it opens a new
//! issue URL through the links service, prefilled (an issue form's fields
//! or the body), percent-encoded, under GitHub's limit with the pick
//! trimmed first.

use std::sync::Arc;

use egui_pop_api::about::report::{self, Report, TRIMMED, URL_LIMIT, describe_user_agent};
use egui_pop_api::about::{self, AboutPlugin, AppInfo, CMD_REPORT};
use egui_pop_api::app::MENU_HELP;
use egui_pop_api::links::{LinksPlugin, RecordingOpener};
use egui_pop_api::{App, AppBuilder};
use egui_pop_model::commands::Origin;
use egui_pop_model::menus::Item;
use serde_json::Value as Json;

const ISSUES: &str = "https://github.com/owner/repo/issues";

fn builder(info: AppInfo) -> AppBuilder {
    App::builder()
        .name("Darkroom")
        .plugin(AboutPlugin::new(info))
}

fn in_help(app: &App) -> bool {
    app.handle()
        .menu(&MENU_HELP)
        .iter()
        .any(|i| matches!(&i.item, Item::Command { id, .. } if *id == CMD_REPORT))
}

fn report(pick: Option<String>) -> Report {
    Report {
        title: "Problem in Files".into(),
        version: "Darkroom 1.2.0\negui-pop 0.3.0\negui 0.37\n".into(),
        platform: "web (wasm32), Firefox 132 on Linux".into(),
        page: Some("Files (story files)".into()),
        pick,
    }
}

#[test]
fn without_an_issues_url_there_is_no_command() {
    let app = builder(AppInfo::new("", "1.2.0"))
        .build(&egui::Context::default())
        .unwrap();
    assert!(app.handle().command(&CMD_REPORT).is_none());
    assert!(!in_help(&app));
    assert!(about::report_url(app.handle()).is_err());
}

#[test]
fn with_an_issues_url_it_is_in_help_and_the_palette() {
    let app = builder(AppInfo::new("", "1.2.0").issues(ISSUES))
        .build(&egui::Context::default())
        .unwrap();
    let h = app.handle();
    assert!(h.diagnostics().is_empty(), "{:?}", h.diagnostics());
    let spec = h.command(&CMD_REPORT).expect("declared");
    assert!(spec.palette);
    assert!(in_help(&app));
    assert_eq!(h.command_menu_title(&spec), "Report a problem");
}

#[test]
fn it_opens_the_new_issue_through_the_links_service() {
    let rec = RecordingOpener::default();
    let mut app = builder(
        AppInfo::new("", "1.2.0")
            .issues(ISSUES)
            .issue_form("bug.yml"),
    )
    .plugin(LinksPlugin::default().opener(Arc::new(rec.clone())))
    .build(&egui::Context::default())
    .unwrap();
    let out = app.run(&CMD_REPORT, Json::Null, Origin::User);
    let v = out.now().unwrap().unwrap();
    let url = v["url"].as_str().unwrap().to_string();
    assert_eq!(v["trimmed"], false);
    let records = rec.records();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].target, url);
    assert!(
        url.starts_with(&format!(
            "{ISSUES}/new?template=bug.yml&title=Problem%20in%20Darkroom&version=Darkroom%201.2.0%0Aegui-pop%20"
        )),
        "{url}"
    );
    assert!(url.contains("&platform="), "{url}");
    // The platform is its own field, not a line of the version info.
    assert!(!url.contains("Platform%3A"), "{url}");
    // No page said, nothing inspected.
    assert!(!url.contains("&page=") && !url.contains("&pick="), "{url}");
}

#[test]
fn the_page_comes_from_the_application() {
    let app = builder(AppInfo::new("", "1.2.0").issues(ISSUES))
        .build(&egui::Context::default())
        .unwrap();
    assert_eq!(about::report_of(app.handle()).unwrap().page, None);
    let app = App::builder()
        .name("Darkroom")
        .plugin(
            AboutPlugin::new(AppInfo::new("", "1.2.0").issues(ISSUES))
                .page(|_| Some("Develop".into())),
        )
        .build(&egui::Context::default())
        .unwrap();
    let r = about::report_of(app.handle()).unwrap();
    assert_eq!(r.page.as_deref(), Some("Develop"));
    assert_eq!(r.title, "Problem in Develop");
}

#[test]
fn an_issue_form_gets_one_parameter_per_field() {
    let url = report(Some(
        "Element: Button \"OK\" (widget)\nRole: Button\n".into(),
    ))
    .url(ISSUES, Some("bug.yml"));
    assert!(!url.trimmed);
    let u = url.url;
    assert!(
        u.starts_with(&format!("{ISSUES}/new?template=bug.yml&title=")),
        "{u}"
    );
    for field in [
        report::FIELD_VERSION,
        report::FIELD_PLATFORM,
        report::FIELD_PAGE,
        report::FIELD_PICK,
    ] {
        assert!(u.contains(&format!("&{field}=")), "{field} in {u}");
    }
    assert!(!u.contains("&body="), "{u}");
    assert!(u.contains("&page=Files%20%28story%20files%29"), "{u}");
    assert!(
        u.contains("&pick=Element%3A%20Button%20%22OK%22%20%28widget%29%0ARole%3A%20Button%0A"),
        "{u}"
    );
    assert!(u.contains("Firefox%20132%20on%20Linux"), "{u}");
}

#[test]
fn without_a_form_everything_goes_in_the_body() {
    let url = report(Some("Role: Button".into())).url(&format!("{ISSUES}/"), None);
    let u = url.url;
    assert!(
        u.starts_with(&format!("{ISSUES}/new?title=Problem%20in%20Files&body=")),
        "{u}"
    );
    assert!(!u.contains("template="), "{u}");
    for section in [
        "What%20happened",
        "Version%20info",
        "Platform",
        "Page",
        "Inspected%20element",
    ] {
        assert!(
            u.contains(&format!("%23%23%23%20{section}%0A")),
            "{section} in {u}"
        );
    }
    // A URL already ending `new` is taken as is.
    let again = report(None).url(&format!("{ISSUES}/new"), None).url;
    assert!(again.starts_with(&format!("{ISSUES}/new?")), "{again}");
}

#[test]
fn every_character_outside_the_unreserved_ones_is_encoded() {
    assert_eq!(report::encode("a-Z_0.~"), "a-Z_0.~");
    assert_eq!(
        report::encode("a b&c=d#e?f/g+h"),
        "a%20b%26c%3Dd%23e%3Ff%2Fg%2Bh"
    );
    assert_eq!(report::encode("é\n×"), "%C3%A9%0A%C3%97");
}

#[test]
fn a_long_pick_is_trimmed_first_and_says_so() {
    let pick: String = (0..2000).map(|i| format!("Row {i}: value\n")).collect();
    for form in [Some("bug.yml"), None] {
        let url = report(Some(pick.clone())).url(ISSUES, form);
        assert!(url.trimmed);
        assert!(url.url.len() <= URL_LIMIT, "{}", url.url.len());
        assert!(url.url.len() > URL_LIMIT - 200, "{}", url.url.len());
        assert!(
            url.url.contains(&report::encode(TRIMMED)),
            "{form:?}: no (trimmed) mark"
        );
        // The page stays whole.
        assert!(url.url.contains("Files%20%28story%20files%29"), "{form:?}");
        assert!(url.url.contains("Row%200%3A%20value"), "{form:?}");
    }
}

#[test]
fn a_long_page_is_trimmed_once_the_pick_is_gone() {
    let mut r = report(Some("Role: Button".into()));
    r.page = Some("é".repeat(5000));
    let url = r.url(ISSUES, Some("bug.yml"));
    assert!(url.trimmed);
    assert!(url.url.len() <= URL_LIMIT, "{}", url.url.len());
    assert!(!url.url.contains("&pick="), "the pick goes first");
    assert!(url.url.contains("&page=%C3%A9"));
    assert!(url.url.ends_with(&report::encode(TRIMMED)));
}

#[test]
fn the_browser_and_its_system_are_read_from_the_user_agent() {
    let cases = [
        (
            "Mozilla/5.0 (X11; Linux x86_64; rv:132.0) Gecko/20100101 Firefox/132.0",
            "Firefox 132 on Linux",
        ),
        (
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
            "Chrome 131 on Windows",
        ),
        (
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 Edg/131.0.2903.70",
            "Edge 131 on Windows",
        ),
        (
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.1 Safari/605.1.15",
            "Safari 18 on macOS",
        ),
        (
            "Mozilla/5.0 (iPhone; CPU iPhone OS 18_1 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.1 Mobile/15E148 Safari/604.1",
            "Safari 18 on iOS",
        ),
        (
            "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Mobile Safari/537.36",
            "Chrome 131 on Android",
        ),
        ("SomeBot/1.0", "SomeBot/1.0"),
    ];
    for (ua, said) in cases {
        assert_eq!(describe_user_agent(ua), said, "{ua}");
    }
}
