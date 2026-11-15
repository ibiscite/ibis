//! Apps Ibis knows. IDs are stored normalized (see [`super::AppId::normalized`]);
//! a test checks that, and that no app appears twice. Add an app only with an
//! ID taken from the app itself (its `CFBundleIdentifier`, or its executable).

use super::{OutputClass, PasteSafety, System};
use crate::contract::citation::KeySyntax;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct App {
    pub system: System,
    pub id: &'static str,
    /// For people reading the table; never shown as the user's data.
    pub name: &'static str,
    /// Known format kind and suggestion; `None` means the kind is unknown.
    pub output: Option<OutputClass>,
    pub safety: PasteSafety,
}

use OutputClass::{Formatted, Key};
use PasteSafety::{NonstandardShortcut, Normal, RemoteDesktop, Terminal, VirtualMachine};
use System::{Macos, Windows};

const fn app(
    system: System,
    id: &'static str,
    name: &'static str,
    output: Option<OutputClass>,
    safety: PasteSafety,
) -> App {
    App {
        system,
        id,
        name,
        output,
        safety,
    }
}

const PANDOC: Option<OutputClass> = Some(Key(KeySyntax::Pandoc));
const BIBLATEX: Option<OutputClass> = Some(Key(KeySyntax::Biblatex));
const FORMATTED: Option<OutputClass> = Some(Formatted);

pub const APPS: &[App] = &[
    // These IDs identify an app, never its current document or web page.
    app(Macos, "com.apple.safari", "Safari", FORMATTED, Normal),
    app(Macos, "com.google.chrome", "Chrome", FORMATTED, Normal),
    app(Macos, "org.chromium.chromium", "Chromium", FORMATTED, Normal),
    app(Macos, "org.mozilla.firefox", "Firefox", FORMATTED, Normal),
    app(Macos, "com.microsoft.edgemac", "Edge", FORMATTED, Normal),
    app(Macos, "com.brave.browser", "Brave", FORMATTED, Normal),
    app(Macos, "com.vivaldi.vivaldi", "Vivaldi", FORMATTED, Normal),
    app(Macos, "company.thebrowser.browser", "Arc", FORMATTED, Normal),
    app(Macos, "com.operasoftware.opera", "Opera", FORMATTED, Normal),
    app(Windows, "chrome.exe", "Chrome", FORMATTED, Normal),
    app(Windows, "chromium.exe", "Chromium", FORMATTED, Normal),
    app(Windows, "firefox.exe", "Firefox", FORMATTED, Normal),
    app(Windows, "msedge.exe", "Edge", FORMATTED, Normal),
    app(Windows, "brave.exe", "Brave", FORMATTED, Normal),
    app(Windows, "vivaldi.exe", "Vivaldi", FORMATTED, Normal),
    app(Windows, "opera.exe", "Opera", FORMATTED, Normal),
    app(Macos, "com.microsoft.vscode", "Visual Studio Code", PANDOC, Normal),
    app(
        Macos,
        "com.microsoft.vscodeinsiders",
        "Visual Studio Code Insiders",
        PANDOC,
        Normal,
    ),
    app(Macos, "com.visualstudio.code.oss", "Code OSS", PANDOC, Normal),
    app(Macos, "com.vscodium", "VSCodium", PANDOC, Normal),
    app(Macos, "com.todesktop.230313mzl4w4u92", "Cursor", PANDOC, Normal),
    app(Macos, "com.sublimetext.4", "Sublime Text", PANDOC, Normal),
    app(Macos, "dev.zed.zed", "Zed", PANDOC, Normal),
    app(Macos, "com.barebones.bbedit", "BBEdit", PANDOC, Normal),
    app(Macos, "com.macromates.textmate", "TextMate", PANDOC, Normal),
    app(Windows, "code.exe", "Visual Studio Code", PANDOC, Normal),
    app(
        Windows,
        "code - insiders.exe",
        "Visual Studio Code Insiders",
        PANDOC,
        Normal,
    ),
    app(Windows, "vscodium.exe", "VSCodium", PANDOC, Normal),
    app(Windows, "cursor.exe", "Cursor", PANDOC, Normal),
    app(Windows, "sublime_text.exe", "Sublime Text", PANDOC, Normal),
    app(Windows, "notepad++.exe", "Notepad++", PANDOC, Normal),
    // Word processors and mail: formatted citations.
    app(Macos, "com.microsoft.word", "Microsoft Word", FORMATTED, Normal),
    app(Windows, "winword.exe", "Microsoft Word", FORMATTED, Normal),
    // The native paste qualification uses Notepad; its plain-text documents
    // keep the person's output choice.
    app(Windows, "notepad.exe", "Notepad", None, Normal),
    app(Macos, "com.apple.iwork.pages", "Pages", FORMATTED, Normal),
    app(Macos, "org.libreoffice.script", "LibreOffice", FORMATTED, Normal),
    app(Windows, "soffice.bin", "LibreOffice", FORMATTED, Normal),
    app(Windows, "soffice.exe", "LibreOffice", FORMATTED, Normal),
    // IDs already used by Zotero discovery and native launching. Delivery is
    // always Copy only; Enter finds and opens instead (decision 0042).
    app(Macos, "org.zotero.zotero", "Zotero", FORMATTED, Normal),
    app(Windows, "zotero.exe", "Zotero", FORMATTED, Normal),
    app(Macos, "com.apple.textedit", "TextEdit", FORMATTED, Normal),
    app(Macos, "com.microsoft.outlook", "Microsoft Outlook", FORMATTED, Normal),
    app(Windows, "outlook.exe", "Microsoft Outlook", FORMATTED, Normal),
    app(Macos, "com.apple.mail", "Mail", FORMATTED, Normal),
    // App metadata and on-device reports; provenance is in decision 0031 (docs/decisions/).
    app(Macos, "com.anthropic.claudefordesktop", "Claude", None, Normal),
    app(Macos, "com.openai.codex", "ChatGPT", None, Normal),
    app(Macos, "com.openai.chat", "ChatGPT Classic", None, Normal),
    app(Macos, "com.apple.notes", "Notes", FORMATTED, Normal),
    app(Macos, "notion.id", "Notion", FORMATTED, Normal),
    app(Macos, "com.lukilabs.lukiapp", "Craft", FORMATTED, Normal),
    app(Macos, "com.microsoft.onenote.mac", "OneNote", FORMATTED, Normal),
    app(Macos, "org.mozilla.thunderbird", "Thunderbird", FORMATTED, Normal),
    // Markdown writing apps: Pandoc citations.
    app(Macos, "md.obsidian", "Obsidian", PANDOC, Normal),
    app(Windows, "obsidian.exe", "Obsidian", PANDOC, Normal),
    app(Macos, "abnerworks.typora", "Typora", PANDOC, Normal),
    app(Windows, "typora.exe", "Typora", PANDOC, Normal),
    app(Macos, "com.zettlr.app", "Zettlr", PANDOC, Normal),
    app(Windows, "zettlr.exe", "Zettlr", PANDOC, Normal),
    app(Macos, "pro.writer.mac", "iA Writer", PANDOC, Normal),
    app(Macos, "com.logseq.logseq", "Logseq", PANDOC, Normal),
    // LaTeX editors: BibLaTeX citations.
    app(Macos, "texshop", "TeXShop", BIBLATEX, Normal),
    app(Windows, "texstudio.exe", "TeXstudio", BIBLATEX, Normal),
    app(Windows, "texworks.exe", "TeXworks", BIBLATEX, Normal),
    app(Windows, "winedt.exe", "WinEdt", BIBLATEX, Normal),
    // Terminals: a paste runs as typed input.
    app(Macos, "com.apple.terminal", "Terminal", None, Terminal),
    app(Macos, "com.googlecode.iterm2", "iTerm2", None, Terminal),
    app(Macos, "net.kovidgoyal.kitty", "kitty", None, Terminal),
    app(Macos, "com.github.wez.wezterm", "WezTerm", None, Terminal),
    app(Macos, "org.alacritty", "Alacritty", None, Terminal),
    app(Macos, "dev.warp.warp-stable", "Warp", None, Terminal),
    app(Macos, "com.mitchellh.ghostty", "Ghostty", None, Terminal),
    app(Macos, "co.zeit.hyper", "Hyper", None, Terminal),
    app(Macos, "org.tabby", "Tabby", None, Terminal),
    app(Macos, "com.termius-dmg.mac", "Termius", None, Terminal),
    app(Windows, "windowsterminal.exe", "Windows Terminal", None, Terminal),
    app(Windows, "cmd.exe", "Command Prompt", None, Terminal),
    app(Windows, "conhost.exe", "Console Window Host", None, Terminal),
    app(Windows, "powershell.exe", "Windows PowerShell", None, Terminal),
    app(Windows, "pwsh.exe", "PowerShell", None, Terminal),
    app(Windows, "wezterm-gui.exe", "WezTerm", None, Terminal),
    app(Windows, "alacritty.exe", "Alacritty", None, Terminal),
    app(Windows, "mintty.exe", "mintty", None, Terminal),
    app(Windows, "putty.exe", "PuTTY", None, Terminal),
    // Their default Ctrl+V does not paste the clipboard.
    app(Windows, "emacs.exe", "Emacs", None, NonstandardShortcut),
    app(Windows, "gvim.exe", "gVim", None, NonstandardShortcut),
    app(Macos, "com.neovide.neovide", "Neovide", None, NonstandardShortcut),
    app(Windows, "neovide.exe", "Neovide", None, NonstandardShortcut),
    // Remote desktops: keystrokes reach another computer.
    app(Macos, "com.microsoft.rdc.macos", "Windows App", None, RemoteDesktop),
    app(Macos, "com.teamviewer.teamviewer", "TeamViewer", None, RemoteDesktop),
    app(Macos, "com.realvnc.vncviewer", "VNC Viewer", None, RemoteDesktop),
    app(Macos, "com.apple.screensharing", "Screen Sharing", None, RemoteDesktop),
    app(Macos, "com.philandro.anydesk", "AnyDesk", None, RemoteDesktop),
    app(Macos, "tv.parsec.www", "Parsec", None, RemoteDesktop),
    app(Macos, "com.p5sys.jump.mac.viewer", "Jump Desktop", None, RemoteDesktop),
    app(
        Macos,
        "com.p5sys.jump.mac.viewer.web",
        "Jump Desktop",
        None,
        RemoteDesktop,
    ),
    app(Macos, "com.nomachine.nxdock", "NoMachine", None, RemoteDesktop),
    app(Macos, "com.omnissa.horizon.client.mac", "Horizon", None, RemoteDesktop),
    app(
        Macos,
        "com.citrix.receiver.icaviewer.mac",
        "Citrix Viewer",
        None,
        RemoteDesktop,
    ),
    app(Windows, "mstsc.exe", "Remote Desktop Connection", None, RemoteDesktop),
    app(Windows, "msrdc.exe", "Remote Desktop", None, RemoteDesktop),
    app(Windows, "teamviewer.exe", "TeamViewer", None, RemoteDesktop),
    app(Windows, "vncviewer.exe", "VNC Viewer", None, RemoteDesktop),
    app(Windows, "anydesk.exe", "AnyDesk", None, RemoteDesktop),
    app(Windows, "wfica32.exe", "Citrix Workspace", None, RemoteDesktop),
    app(Windows, "parsecd.exe", "Parsec", None, RemoteDesktop),
    app(Windows, "nxplayer.exe", "NoMachine", None, RemoteDesktop),
    app(Windows, "nxplayer.bin", "NoMachine", None, RemoteDesktop),
    app(Windows, "vmware-view.exe", "Horizon", None, RemoteDesktop),
    // Virtual machines: keystrokes reach a guest system.
    app(
        Macos,
        "com.parallels.desktop.console",
        "Parallels Desktop",
        None,
        VirtualMachine,
    ),
    app(Macos, "com.vmware.fusion", "VMware Fusion", None, VirtualMachine),
    app(Macos, "com.utmapp.utm", "UTM", None, VirtualMachine),
    app(
        Macos,
        "org.virtualbox.app.virtualboxvm",
        "VirtualBox",
        None,
        VirtualMachine,
    ),
    app(Windows, "vmware.exe", "VMware Workstation", None, VirtualMachine),
    app(Windows, "vmconnect.exe", "Hyper-V", None, VirtualMachine),
    app(Windows, "virtualboxvm.exe", "VirtualBox", None, VirtualMachine),
];
