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
    /// `None` keeps the user's default output.
    pub output: Option<OutputClass>,
    pub safety: PasteSafety,
}

use OutputClass::{Formatted, Key};
use PasteSafety::{Normal, RemoteDesktop, Terminal, VirtualMachine};
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
    // Word processors and mail: formatted citations.
    app(Macos, "com.microsoft.word", "Microsoft Word", FORMATTED, Normal),
    app(Windows, "winword.exe", "Microsoft Word", FORMATTED, Normal),
    app(Macos, "com.apple.iwork.pages", "Pages", FORMATTED, Normal),
    app(Macos, "org.libreoffice.script", "LibreOffice", FORMATTED, Normal),
    app(Windows, "soffice.bin", "LibreOffice", FORMATTED, Normal),
    app(Windows, "soffice.exe", "LibreOffice", FORMATTED, Normal),
    app(Macos, "com.apple.textedit", "TextEdit", FORMATTED, Normal),
    app(Macos, "com.microsoft.outlook", "Microsoft Outlook", FORMATTED, Normal),
    app(Windows, "outlook.exe", "Microsoft Outlook", FORMATTED, Normal),
    app(Macos, "com.apple.mail", "Mail", FORMATTED, Normal),
    // Markdown writing apps: Pandoc citations.
    app(Macos, "md.obsidian", "Obsidian", PANDOC, Normal),
    app(Windows, "obsidian.exe", "Obsidian", PANDOC, Normal),
    app(Macos, "abnerworks.typora", "Typora", PANDOC, Normal),
    app(Windows, "typora.exe", "Typora", PANDOC, Normal),
    app(Macos, "com.zettlr.app", "Zettlr", PANDOC, Normal),
    app(Windows, "zettlr.exe", "Zettlr", PANDOC, Normal),
    app(Macos, "pro.writer.mac", "iA Writer", PANDOC, Normal),
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
    app(Windows, "windowsterminal.exe", "Windows Terminal", None, Terminal),
    app(Windows, "cmd.exe", "Command Prompt", None, Terminal),
    app(Windows, "conhost.exe", "Console Window Host", None, Terminal),
    app(Windows, "powershell.exe", "Windows PowerShell", None, Terminal),
    app(Windows, "pwsh.exe", "PowerShell", None, Terminal),
    app(Windows, "wezterm-gui.exe", "WezTerm", None, Terminal),
    app(Windows, "alacritty.exe", "Alacritty", None, Terminal),
    app(Windows, "mintty.exe", "mintty", None, Terminal),
    app(Windows, "putty.exe", "PuTTY", None, Terminal),
    // Remote desktops: keystrokes reach another computer.
    app(Macos, "com.microsoft.rdc.macos", "Windows App", None, RemoteDesktop),
    app(Macos, "com.teamviewer.teamviewer", "TeamViewer", None, RemoteDesktop),
    app(Macos, "com.realvnc.vncviewer", "VNC Viewer", None, RemoteDesktop),
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
