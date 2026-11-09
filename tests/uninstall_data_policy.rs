#![cfg(windows)]

use moodflow_installer::{manifest::AppManifest, meta, uninstaller};

#[test]
fn silent_data_deletion_is_explicit_and_keep_wins_in_either_order() {
    meta::init(
        AppManifest::from_toml_bytes(
            br#"
[app]
id = "Fixture"
display_name = "Fixture App"
version = "1.0.0"
publisher = "Fixture"
main_exe = "fixture.exe"
"#,
        )
        .unwrap(),
    );
    for (flags, expected_delete) in [
        (vec!["--silent"], false),
        (vec!["--silent", "--keep-user-data"], false),
        (vec!["--silent", "--delete-user-data"], true),
        (
            vec!["--silent", "--delete-user-data", "--keep-user-data"],
            false,
        ),
        (
            vec!["--silent", "--keep-user-data", "--delete-user-data"],
            false,
        ),
    ] {
        let args = uninstaller::args::parse(&flags);
        let options =
            uninstaller::silent_cleanup_options(args.keep_user_data, args.delete_user_data);
        assert_eq!(options.clean_roaming, expected_delete, "{flags:?}");
        assert_eq!(options.keep_user_data, args.keep_user_data);
    }
}
