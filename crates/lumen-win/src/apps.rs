//! The apps installed on this PC, as the Start menu's "All apps" list has
//! them: classic programs and Store apps alike. Read from the shell's Apps
//! folder through the public shell API.

use windows::{
    Win32::{
        Foundation::RPC_E_CHANGED_MODE,
        System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree},
        UI::Shell::{BHID_EnumItems, FOLDERID_AppsFolder, IEnumShellItems, IShellItem, KF_FLAG_DEFAULT, SHGetKnownFolderItem, SIGDN, SIGDN_NORMALDISPLAY, SIGDN_PARENTRELATIVEPARSING},
    },
    core::PWSTR,
};

/// One installed app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    pub name: String,
    /// What to open, and where to ask for its icon: `shell:AppsFolder\<id>`.
    pub target: String,
}

/// Every app in the Apps folder, sorted by name. Empty if the shell cannot be asked.
pub fn installed() -> Vec<App> {
    let mut apps = read().unwrap_or_default();
    apps.retain(|app| is_worth_listing(&app.name));
    apps.sort_by_cached_key(|app| app.name.to_lowercase());
    apps.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
    apps
}

fn read() -> Option<Vec<App>> {
    unsafe {
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if hr.is_err() && hr != RPC_E_CHANGED_MODE {
            return None;
        }
        let folder: IShellItem = SHGetKnownFolderItem(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None).ok()?;
        let items: IEnumShellItems = folder.BindToHandler(None, &BHID_EnumItems).ok()?;
        let mut apps = Vec::new();
        loop {
            let mut item = [None];
            let mut fetched = 0;
            if items.Next(&mut item, Some(&mut fetched)).is_err() || fetched == 0 {
                break;
            }
            let Some(item) = item[0].take() else { break };
            if let (Some(name), Some(id)) = (display_name(&item, SIGDN_NORMALDISPLAY), display_name(&item, SIGDN_PARENTRELATIVEPARSING))
                && !name.is_empty()
                && !id.is_empty()
            {
                apps.push(App { name, target: format!(r"shell:AppsFolder\{id}") });
            }
        }
        Some(apps)
    }
}

unsafe fn display_name(item: &IShellItem, kind: SIGDN) -> Option<String> {
    unsafe {
        let raw: PWSTR = item.GetDisplayName(kind).ok()?;
        let text = raw.to_string().ok();
        CoTaskMemFree(Some(raw.0 as *const _));
        text
    }
}

/// The Apps folder also holds uninstallers, read-me files and web links that
/// installers leave in the Start menu.
fn is_worth_listing(name: &str) -> bool {
    let lower = name.to_lowercase();
    let junk_start = ["uninstall", "remove ", "readme", "read me", "release notes", "licence", "license", "changelog"];
    let junk_end = [" website", " web site", " documentation", " manual", " uninstaller", " readme", " on the web"];
    !junk_start.iter().any(|junk| lower.starts_with(junk)) && !junk_end.iter().any(|junk| lower.ends_with(junk))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installer_leftovers_are_left_out() {
        for junk in ["Uninstall Foo", "Foo Uninstaller", "README", "Foo Website", "Foo Documentation", "License"] {
            assert!(!is_worth_listing(junk), "{junk}");
        }
        for app in ["Notepad", "Remote Desktop Connection", "Get Help", "Helpdesk Tool"] {
            assert!(is_worth_listing(app), "{app}");
        }
    }

    #[test]
    fn this_pc_has_apps_sorted_and_without_duplicates() {
        let apps = installed();
        // A build machine without a shell would return nothing; any real Windows has dozens.
        if apps.is_empty() {
            return;
        }
        assert!(apps.len() > 5, "only {} apps", apps.len());
        assert!(apps.iter().all(|app| app.target.starts_with(r"shell:AppsFolder\")));
        assert!(apps.windows(2).all(|pair| pair[0].name.to_lowercase() < pair[1].name.to_lowercase()), "sorted, no duplicates");
    }
}
