#![allow(dead_code)]
#[allow(dead_code)]
#[path = "../src/editors.rs"]
mod editors;

use std::fs;

#[test]
fn macos_discovery_finds_only_supported_adobe_app_bundles() {
    let temp = tempfile::tempdir().expect("temp directory");
    let applications = temp.path().join("Applications");
    fs::create_dir_all(applications.join("Adobe Photoshop 2025/Adobe Photoshop 2025.app"))
        .expect("photoshop app");
    fs::create_dir_all(applications.join("Adobe Lightroom Classic/Adobe Lightroom Classic.app"))
        .expect("lightroom app");
    fs::create_dir_all(applications.join("Unrelated Editor.app")).expect("other app");

    let discovered = editors::discover_in(&[applications], editors::EditorPlatform::Macos);
    assert_eq!(discovered.len(), 3);
    assert_eq!(discovered[0].id, "system");
    assert!(discovered.iter().any(|editor| editor.kind == "photoshop"));
    assert!(
        discovered
            .iter()
            .any(|editor| editor.kind == "lightroomClassic")
    );
    assert!(
        discovered
            .iter()
            .all(|editor| !editor.label.contains("Unrelated"))
    );
}

#[test]
fn windows_discovery_uses_known_executable_names() {
    let temp = tempfile::tempdir().expect("temp directory");
    let adobe = temp.path().join("Adobe");
    let photoshop = adobe.join("Photoshop/Photoshop.exe");
    let lightroom = adobe.join("Lightroom/Lightroom.exe");
    fs::create_dir_all(photoshop.parent().expect("photoshop parent")).expect("photoshop dir");
    fs::create_dir_all(lightroom.parent().expect("lightroom parent")).expect("lightroom dir");
    fs::write(&photoshop, b"exe").expect("photoshop executable");
    fs::write(&lightroom, b"exe").expect("lightroom executable");
    fs::write(adobe.join("Photoshop/not-photoshop.exe"), b"exe").expect("other executable");

    let discovered = editors::discover_in(&[adobe], editors::EditorPlatform::Windows);
    assert_eq!(discovered.len(), 3);
    assert!(discovered.iter().any(|editor| editor.kind == "photoshop"));
    assert!(
        discovered
            .iter()
            .any(|editor| editor.kind == "lightroomClassic")
    );
}
