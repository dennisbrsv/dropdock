use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    io::{BufRead, BufReader, Read},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc::Sender,
    time::{SystemTime, UNIX_EPOCH},
};
type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Source {
    Pacman,
    FlatpakUser,
    FlatpakSystem,
    Snap,
    Steam,
    Local,
    Shortcut,
}
impl Source {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pacman => "Arch package",
            Self::FlatpakUser => "Flatpak · user",
            Self::FlatpakSystem => "Flatpak · system",
            Self::Snap => "Snap",
            Self::Steam => "Steam",
            Self::Local => "Dropdock",
            Self::Shortcut => "External launcher",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct App {
    pub id: String,
    pub name: String,
    pub version: String,
    pub source: Source,
    pub desktop: Option<PathBuf>,
    pub is_app: bool,
    pub description: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Deb,
    Arch,
    AppImage,
    FlatpakRef,
    FlatpakBundle,
    Cargo,
    Pkgbuild,
    Portable,
    Rpm,
    Unknown,
}
#[derive(Clone, Debug)]
pub struct Package {
    pub path: PathBuf,
    pub name: String,
    pub kind: Kind,
    pub detail: String,
    pub hash: String,
}
impl Package {
    pub fn label(&self) -> &'static str {
        match self.kind {
            Kind::Deb => "DEBIAN PACKAGE",
            Kind::Arch => "ARCH PACKAGE",
            Kind::AppImage => "APPIMAGE",
            Kind::FlatpakRef | Kind::FlatpakBundle => "FLATPAK",
            Kind::Cargo => "RUST PROJECT",
            Kind::Pkgbuild => "ARCH BUILD RECIPE",
            Kind::Portable => "PORTABLE APPLICATION",
            Kind::Rpm => "RPM PACKAGE",
            Kind::Unknown => "UNRECOGNIZED",
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct Local {
    id: String,
    name: String,
    directory: PathBuf,
    desktop: PathBuf,
}
#[derive(Clone)]
pub enum Event {
    Log(String),
    Inventory(Vec<App>),
    InventoryError(String),
    Prepared(Vec<Package>),
    Inspected(Vec<Package>, Vec<String>),
    Success(String),
    Error(String),
    InstallReview(Package, String),
    RemoveReview(App, String),
    SearchResults(String),
}
pub fn home() -> PathBuf {
    #[cfg(test)]
    {
        static ROOT: std::sync::LazyLock<PathBuf> = std::sync::LazyLock::new(|| {
            let p = std::env::temp_dir().join(format!("dropdock-tests-{}", std::process::id()));
            fs::create_dir_all(&p).unwrap();
            p
        });
        ROOT.clone()
    }
    #[cfg(not(test))]
    {
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
    }
}
pub fn operation_lock() -> Result<fs::File> {
    fs::create_dir_all(data()).map_err(|e| e.to_string())?;
    let f = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(data().join("operation.lock"))
        .map_err(|e| e.to_string())?;
    f.try_lock()
        .map_err(|_| "Another Dropdock operation is already running.".to_string())?;
    Ok(f)
}
pub fn data() -> PathBuf {
    home().join(".local/share/dropdock")
}
fn state() -> Result<Vec<Local>> {
    let p = data().join("installed.json");
    if !p.exists() {
        return Ok(vec![]);
    }
    serde_json::from_slice(&fs::read(p).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
fn save(v: &[Local]) -> Result<()> {
    fs::create_dir_all(data()).map_err(|e| e.to_string())?;
    let p = data().join("installed.json.tmp");
    fs::write(&p, serde_json::to_vec_pretty(v).unwrap()).map_err(|e| e.to_string())?;
    fs::rename(p, data().join("installed.json")).map_err(|e| e.to_string())
}
fn output(program: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
pub fn clean_log(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.next() == Some('[') {
                for x in chars.by_ref() {
                    if ('@'..='~').contains(&x) {
                        break;
                    }
                }
            }
        } else if c != '\r' {
            out.push(c)
        }
    }
    out
}
fn command(program: &str, args: &[String], cwd: Option<&Path>, tx: &Sender<Event>) -> Result<()> {
    tx.send(Event::Log(format!(
        "› {} {}",
        program,
        args.iter()
            .map(|a| format!("{a:?}"))
            .collect::<Vec<_>>()
            .join(" ")
    )))
    .ok();
    let mut cmd = Command::new(program);
    cmd.args(args)
        .env("LC_ALL", "C")
        .env("TERM", "dumb")
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env(
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER",
            "/usr/bin/cc",
        )
        .env("CC", "/usr/bin/cc")
        .env("CXX", "/usr/bin/c++")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Could not start {program}: {e}"))?;
    let a = child.stdout.take().unwrap();
    let b = child.stderr.take().unwrap();
    let t = tx.clone();
    let t2 = tx.clone();
    let j1 = std::thread::spawn(move || {
        let mut diagnostics = Vec::new();
        for s in BufReader::new(a).lines().map_while(std::result::Result::ok) {
            if (s.contains("breaks dependency") || s.contains("failed to prepare transaction"))
                && diagnostics.len() < 40
            {
                diagnostics.push(s.clone());
            }
            t.send(Event::Log(clean_log(&s))).ok();
        }
        diagnostics
    });
    let j2 = std::thread::spawn(move || {
        let mut diagnostics = Vec::new();
        for s in BufReader::new(b).lines().map_while(std::result::Result::ok) {
            if (s.contains("breaks dependency") || s.contains("failed to prepare transaction"))
                && diagnostics.len() < 40
            {
                diagnostics.push(s.clone());
            }
            t2.send(Event::Log(clean_log(&s))).ok();
        }
        diagnostics
    });
    let status = child.wait().map_err(|e| e.to_string())?;
    let mut diagnostics = j1.join().unwrap_or_default();
    diagnostics.extend(j2.join().unwrap_or_default());
    if status.success() {
        Ok(())
    } else if let Some(message) = dependency_failure(program, args, &diagnostics) {
        Err(message)
    } else {
        Err(format!("{program} did not complete ({}). See the activity log. Nothing is marked successful; a package operation may have partially completed.",status))
    }
}
fn dependency_failure(program: &str, args: &[String], lines: &[String]) -> Option<String> {
    let pacman = program == "/usr/bin/pacman"
        || (program == "/usr/bin/pkexec" && args.first().is_some_and(|a| a == "/usr/bin/pacman"));
    if !pacman
        || !args.iter().any(|a| a == "-R")
        || !lines
            .iter()
            .any(|l| l.contains("failed to prepare transaction (could not satisfy dependencies)"))
    {
        return None;
    }
    let reasons: Vec<_> = lines
        .iter()
        .filter(|l| l.contains("breaks dependency"))
        .map(|l| l.trim_start_matches(":: "))
        .collect();
    if reasons.is_empty() {
        return None;
    }
    Some(format!("Cannot uninstall: other installed packages need this package. No packages were removed.\n\n{}\n\nKeep it installed while these packages depend on it.", reasons.join("\n")))
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}
fn field(s: &str, k: &str) -> String {
    let mut sec = false;
    for l in s.lines() {
        if l.starts_with('[') {
            sec = l == "[Desktop Entry]";
        } else if sec {
            if let Some(v) = l.strip_prefix(&format!("{k}=")) {
                return v.to_string();
            }
        }
    }
    String::new()
}
fn desc(s: &str, k: &str) -> String {
    s.split(&format!("%{k}%\n"))
        .nth(1)
        .unwrap_or("")
        .split("\n\n")
        .next()
        .unwrap_or("")
        .trim()
        .to_string()
}
pub fn inventory() -> Result<Vec<App>> {
    let mut apps = BTreeMap::<String, App>::new();
    let mut owners = HashMap::new();
    for entry in fs::read_dir("/var/lib/pacman/local")
        .into_iter()
        .flatten()
        .flatten()
    {
        let text = fs::read_to_string(entry.path().join("desc")).unwrap_or_default();
        let id = desc(&text, "NAME");
        if id.is_empty() {
            continue;
        }
        let files = fs::read_to_string(entry.path().join("files")).unwrap_or_default();
        for f in files
            .lines()
            .filter(|l| l.starts_with("usr/share/applications/") && l.ends_with(".desktop"))
        {
            owners.insert(PathBuf::from("/").join(f), id.clone());
        }
        apps.insert(
            format!("pacman:{id}"),
            App {
                id: id.clone(),
                name: id,
                version: desc(&text, "VERSION"),
                source: Source::Pacman,
                desktop: None,
                is_app: false,
                description: desc(&text, "DESC"),
            },
        );
    }
    let mut hidden_ids = HashSet::new();
    for dir in [
        home().join(".local/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
        PathBuf::from("/usr/share/applications"),
    ] {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|x| x.to_str()) != Some("desktop") {
                    continue;
                }
                let desktop_id = p.file_name().unwrap().to_string_lossy().to_string();
                if !hidden_ids.insert(desktop_id) {
                    continue;
                }
                let text = fs::read_to_string(&p).unwrap_or_default();
                if field(&text, "Hidden") == "true"
                    || field(&text, "NoDisplay") == "true"
                    || field(&text, "Type") != "Application"
                {
                    continue;
                }
                let name = field(&text, "Name");
                if name.is_empty() {
                    continue;
                }
                let owner = owners.get(&p).cloned().or_else(|| {
                    owners
                        .get(&PathBuf::from("/usr/share/applications").join(p.file_name().unwrap()))
                        .cloned()
                });
                if let Some(id) = owner {
                    if let Some(a) = apps.get_mut(&format!("pacman:{id}")) {
                        a.name = name;
                        a.desktop = Some(p);
                        a.is_app = true;
                    }
                } else if let Some(id) = field(&text, "Exec")
                    .split("steam://rungameid/")
                    .nth(1)
                    .map(|s| s.split_whitespace().next().unwrap_or("").to_string())
                    .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
                {
                    apps.insert(
                        format!("steam:{id}"),
                        App {
                            id,
                            name,
                            version: String::new(),
                            source: Source::Steam,
                            desktop: Some(p),
                            is_app: true,
                            description:
                                "Steam game launcher. Uninstall opens Steam's own removal dialog."
                                    .into(),
                        },
                    );
                } else if field(&text, "X-Dropdock-Managed") != "true" {
                    apps.insert(format!("shortcut:{}",p.display()),App{id:p.to_string_lossy().into_owned(),name,version:String::new(),source:Source::Shortcut,desktop:Some(p),is_app:true,description:"Not owned by pacman or Dropdock. Manage it with its original installer.".into()});
                }
            }
        }
    }
    for (flag, source) in [
        ("--user", Source::FlatpakUser),
        ("--system", Source::FlatpakSystem),
    ] {
        if let Ok(text) = output(
            "/usr/bin/flatpak",
            &[
                "list",
                flag,
                "--app",
                "--columns=application,name,version,branch,arch",
            ],
        ) {
            for line in text.lines() {
                let f: Vec<_> = line.split('\t').collect();
                if f.len() < 5 {
                    continue;
                }
                let id = format!("{}/{}/{}", f[0], f[4], f[3]);
                apps.insert(
                    format!("{flag}:{id}"),
                    App {
                        id,
                        name: f[1].into(),
                        version: f[2].into(),
                        source: source.clone(),
                        desktop: None,
                        is_app: true,
                        description: f[0].into(),
                    },
                );
            }
        }
    }
    if Path::new("/usr/bin/snap").exists() {
        if let Ok(text) = output("/usr/bin/snap", &["list"]) {
            for line in text.lines().skip(1) {
                let fields: Vec<_> = line.split_whitespace().collect();
                if fields.len() < 6 {
                    continue;
                }
                let id = fields[0].to_string();
                let mut desktop = None;
                let mut name = id.clone();
                if let Ok(entries) = fs::read_dir("/var/lib/snapd/desktop/applications") {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if p.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .starts_with(&format!("{id}_"))
                        {
                            let text = fs::read_to_string(&p).unwrap_or_default();
                            if field(&text, "NoDisplay") != "true" {
                                name = field(&text, "Name");
                                desktop = Some(p);
                                break;
                            }
                        }
                    }
                }
                apps.retain(|_, a| {
                    !(a.source == Source::Shortcut
                        && a.desktop.as_ref().is_some_and(|p| {
                            let t = fs::read_to_string(p).unwrap_or_default();
                            field(&t, "Exec")
                                .split_whitespace()
                                .next()
                                .is_some_and(|x| {
                                    x == format!("/var/lib/snapd/snap/bin/{id}")
                                        || x == format!("/snap/bin/{id}")
                                })
                        }))
                });
                apps.insert(
                    format!("snap:{id}"),
                    App {
                        id,
                        name,
                        version: fields[1].into(),
                        source: Source::Snap,
                        is_app: desktop.is_some(),
                        desktop,
                        description: "Installed through Snap".into(),
                    },
                );
            }
        }
    }
    for local in state()? {
        apps.insert(
            format!("local:{}", local.id),
            App {
                id: local.id,
                name: local.name,
                version: "Local".into(),
                source: Source::Local,
                desktop: Some(local.desktop),
                is_app: true,
                description: "Installed and tracked by Dropdock".into(),
            },
        );
    }
    let mut v: Vec<_> = apps.into_values().collect();
    v.sort_by_key(|a| a.name.to_lowercase());
    Ok(v)
}
fn sha(path: &Path) -> Result<String> {
    let mut f = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut h = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}
fn kind(path: &Path) -> Kind {
    if path.is_dir() {
        if path.join("PKGBUILD").is_file() {
            return Kind::Pkgbuild;
        }
        if path.join("Cargo.toml").is_file() {
            return Kind::Cargo;
        }
        return Kind::Portable;
    }
    let n = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    if n.ends_with(".deb") {
        Kind::Deb
    } else if n.contains(".pkg.tar.") {
        Kind::Arch
    } else if n.ends_with(".appimage") {
        Kind::AppImage
    } else if n.ends_with(".flatpakref") {
        Kind::FlatpakRef
    } else if n.ends_with(".flatpak") {
        Kind::FlatpakBundle
    } else if n.ends_with(".rpm") {
        Kind::Rpm
    } else {
        Kind::Unknown
    }
}
fn require_arch() -> Result<()> {
    if Path::new("/var/lib/pacman/local").is_dir() && Path::new("/usr/bin/pacman").is_file() {
        Ok(())
    } else {
        Err("Native package management and Debian conversion currently require Arch Linux or an Arch-based distribution. Portable apps, Cargo projects and Flatpak are available on other distributions.".into())
    }
}
pub fn inspect(path: PathBuf) -> Result<Package> {
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    let k = kind(&path);
    if matches!(k, Kind::Deb | Kind::Arch | Kind::Pkgbuild | Kind::Rpm) {
        require_arch()?;
    }
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let mut detail=match k {
        Kind::Deb=>"Convert with debtap, then review the generated Arch package before installing. Conversion does not guarantee binary compatibility. Only install packages from publishers you trust.".into(),
        Kind::Arch=>output("/usr/bin/pacman",&["-Qip",path.to_str().ok_or("Non-UTF-8 path")?])?,
        Kind::AppImage=>{let mut f=fs::File::open(&path).map_err(|e|e.to_string())?;let mut magic=[0u8;11];f.read_exact(&mut magic).map_err(|e|e.to_string())?;if &magic[..4]!=b"\x7fELF"||&magic[8..10]!=b"AI"{return Err("This file does not have an AppImage header.".into())}"Install a private copy and add an application-menu launcher. No administrator password needed. Updates are manual; some AppImages require FUSE.".into()},
        Kind::FlatpakRef|Kind::FlatpakBundle=>"Install with Flatpak for your user account. Flatpak will show its permissions and transaction in a terminal for your confirmation.".into(),
        Kind::Cargo=>"Build this Rust project with cargo in a private workspace. Build scripts and dependencies execute code as your user. Only build projects you trust. Generated binaries will be offered for installation.".into(),
        Kind::Pkgbuild=>"Run makepkg as your user in a private workspace, then review the output package. PKGBUILD is executable code: only build trusted recipes. Missing build dependencies must be installed separately.".into(),
        Kind::Portable=>"Import a portable application folder. Select its main executable after inspection; the complete folder will be copied. Generic source repositories without Cargo.toml or PKGBUILD cannot be built automatically.".into(),
        Kind::Rpm=>"RPM cannot be reliably converted by debtap. Search your enabled Arch repositories for a compatible package below. Dropdock will not relabel an RPM as an Arch package or silently install incompatible dependencies.".into(),
        Kind::Unknown=>"Supported: .deb, .pkg.tar.zst/.xz/.gz, AppImage, .flatpakref, .flatpak, portable folders, Cargo projects and PKGBUILD folders. RPM files can be matched against Arch repositories.".into()
    };
    if k == Kind::Arch {
        let script = output(
            "/usr/bin/bsdtar",
            &["-xOf", path.to_str().unwrap(), ".INSTALL"],
        )
        .unwrap_or_default();
        if !script.is_empty() {
            detail.push_str("\n\nPACKAGE INSTALL SCRIPT (runs with administrator privileges):\n");
            detail.push_str(&script);
        }
    }
    let hash = if path.is_file() {
        sha(&path)?
    } else {
        String::new()
    };
    Ok(Package {
        path,
        name,
        kind: k,
        detail,
        hash,
    })
}
fn workspace() -> Result<PathBuf> {
    let p = data().join("work").join(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_string(),
    );
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}
fn check(p: &Package) -> Result<()> {
    if p.path.is_file() && sha(&p.path)? != p.hash {
        Err("The file changed after inspection. Drop it again to review the new version.".into())
    } else {
        Ok(())
    }
}
fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
    copy_tree_mode(src, dst, false)
}
fn copy_tree_mode(src: &Path, dst: &Path, building: bool) -> Result<()> {
    if dst.starts_with(src) {
        return Err(
            "Choose a specific application folder, not a parent of Dropdock's data directory."
                .into(),
        );
    }
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    let entries = walkdir::WalkDir::new(src)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            e.depth() == 0
                || !(e.file_name() == ".git"
                    || (building
                        && matches!(e.file_name().to_str(), Some("target" | "node_modules"))))
        });
    for entry in entries {
        let e = entry.map_err(|e| e.to_string())?;
        let rel = e.path().strip_prefix(src).map_err(|e| e.to_string())?;
        if rel.as_os_str().is_empty() {
            continue;
        }
        let to = dst.join(rel);
        if e.file_type().is_symlink() {
            let resolved = e.path().canonicalize().map_err(|e| e.to_string())?;
            if !resolved.starts_with(src) {
                return Err(format!(
                    "External symbolic link is not portable: {}",
                    e.path().display()
                ));
            }
            let target = fs::read_link(e.path()).map_err(|e| e.to_string())?;
            let target = if target.is_absolute() {
                dst.join(resolved.strip_prefix(src).unwrap())
            } else {
                target
            };
            std::os::unix::fs::symlink(target, to).map_err(|e| e.to_string())?;
        } else if e.file_type().is_dir() {
            fs::create_dir_all(to).map_err(|e| e.to_string())?;
        } else if e.file_type().is_file() {
            fs::copy(e.path(), to).map_err(|e| e.to_string())?;
        } else {
            return Err("Special files cannot be imported.".into());
        }
    }
    Ok(())
}
pub fn prepare(p: Package, tx: &Sender<Event>) -> Result<Vec<Package>> {
    check(&p)?;
    let dir = workspace()?;
    match p.kind {
        Kind::Deb => {
            let src = dir.join("input.deb");
            fs::copy(&p.path, &src).map_err(|e| e.to_string())?;
            let text = fs::read_to_string("/usr/bin/debtap")
                .map_err(|e| format!("Install debtap first: {e}"))?;
            // debtap 3.6.3 rejects current uncompressed pkgfile cache names. Correct
            // only that exact check in a private copy; do not alter system files.
            let broken = "grep -E '*.files?(.[[:digit:]]{3})'";
            let program = if text.contains(broken) {
                let local = dir.join("debtap-compat");
                fs::write(
                    &local,
                    text.replace(broken, "grep -E '\\.files(\\.[[:digit:]]{3})?$'"),
                )
                .map_err(|e| e.to_string())?;
                fs::set_permissions(&local, fs::Permissions::from_mode(0o700))
                    .map_err(|e| e.to_string())?;
                tx.send(Event::Log(
                    "Using a private debtap compatibility fix for current pkgfile cache names."
                        .into(),
                ))
                .ok();
                local
            } else {
                PathBuf::from("/usr/bin/debtap")
            };
            command(
                program.to_str().unwrap(),
                &strings(&["-Q", "-o", dir.to_str().unwrap(), src.to_str().unwrap()]),
                Some(&dir),
                tx,
            )?;
        }
        Kind::Pkgbuild => {
            copy_tree_mode(&p.path, &dir, true)?;
            command(
                "/usr/bin/makepkg",
                &strings(&["--cleanbuild", "--force", "--noconfirm"]),
                Some(&dir),
                tx,
            )?;
        }
        Kind::Cargo => {
            copy_tree_mode(&p.path, &dir, true)?;
            let mut args = strings(&["build", "--release"]);
            if dir.join("Cargo.lock").exists() {
                args.push("--locked".into())
            }
            command("/usr/bin/cargo", &args, Some(&dir), tx)?;
            let text = output(
                "/usr/bin/cargo",
                &[
                    "metadata",
                    "--no-deps",
                    "--format-version",
                    "1",
                    "--manifest-path",
                    dir.join("Cargo.toml").to_str().unwrap(),
                ],
            )?;
            let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            let mut result = vec![];
            let target = PathBuf::from(
                json["target_directory"]
                    .as_str()
                    .ok_or("Cargo did not report target directory")?,
            )
            .join("release");
            for pkg in json["packages"].as_array().ok_or("No Cargo packages")? {
                for t in pkg["targets"].as_array().into_iter().flatten() {
                    if t["kind"]
                        .as_array()
                        .is_some_and(|k| k.iter().any(|v| v == "bin"))
                    {
                        let name = t["name"].as_str().unwrap_or("");
                        let binary = target.join(name);
                        if binary.is_file() {
                            result.push(Package{hash:sha(&binary)?,path:binary,name:name.into(),kind:Kind::Portable,detail:"Built Rust executable. Install a private copy and create a launcher. If this is a command-line tool, enable ‘Run in terminal’.".into()});
                        }
                    }
                }
            }
            if result.is_empty() {
                return Err("Build completed, but no runnable binary was found. This may be a library project.".into());
            }
            return Ok(result);
        }
        _ => return Err("This item does not need conversion.".into()),
    }
    let mut result = vec![];
    for e in fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten() {
        if kind(&e.path()) == Kind::Arch {
            result.push(inspect(e.path())?);
        }
    }
    if result.is_empty() {
        Err("No Arch package was produced. Check the conversion/build log.".into())
    } else {
        Ok(result)
    }
}
pub fn executable_choices(p: &Path) -> Vec<PathBuf> {
    if !p.is_dir() {
        return vec![p.to_path_buf()];
    }
    walkdir::WalkDir::new(p)
        .max_depth(4)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_file()
                && e.metadata()
                    .is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
        })
        .map(|e| e.path().to_path_buf())
        .take(100)
        .collect()
}
fn exec_quote(p: &Path) -> Result<String> {
    let s = p.to_str().ok_or("Non-UTF-8 executable path")?;
    if s.contains(['\n', '\r']) {
        return Err("Newlines in executable paths are unsupported.".into());
    }
    Ok(format!(
        "\"{}\"",
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('`', "\\`")
            .replace('$', "\\$")
            .replace('%', "%%")
    ))
}
pub fn install(
    p: Package,
    executable: Option<PathBuf>,
    terminal: bool,
    tx: &Sender<Event>,
) -> Result<()> {
    check(&p)?;
    match p.kind {
        Kind::Arch => command(
            "/usr/bin/pkexec",
            &strings(&[
                "/usr/bin/pacman",
                "-U",
                "--noconfirm",
                "--",
                p.path.to_str().unwrap(),
            ]),
            None,
            tx,
        ),
        Kind::FlatpakRef | Kind::FlatpakBundle => {
            // A real terminal preserves Flatpak's dependency and permission confirmation.
            let before = output(
                "/usr/bin/flatpak",
                &["list", "--user", "--app", "--columns=ref,active"],
            )?;
            command(
                "/usr/bin/konsole",
                &strings(&[
                    "--separate",
                    "--nofork",
                    "-e",
                    "/usr/bin/flatpak",
                    "install",
                    "--user",
                    if p.kind == Kind::FlatpakRef {
                        "--from"
                    } else {
                        "--bundle"
                    },
                    p.path.to_str().unwrap(),
                ]),
                None,
                tx,
            )?;
            let after = output(
                "/usr/bin/flatpak",
                &["list", "--user", "--app", "--columns=ref,active"],
            )?;
            if before == after {
                Err("Flatpak made no application changes. The operation may have been cancelled, or the app was already installed.".into())
            } else {
                Ok(())
            }
        }
        Kind::AppImage | Kind::Portable => {
            let selected = executable.unwrap_or_else(|| p.path.clone());
            let selected = selected.canonicalize().map_err(|e| e.to_string())?;
            if p.path.is_dir() && (!selected.starts_with(&p.path) || !selected.is_file()) {
                return Err("Choose an executable inside the application folder.".into());
            }
            let mut records = state()?;
            let id = format!(
                "app-{}",
                &format!(
                    "{:x}",
                    Sha256::digest(p.path.as_os_str().as_encoded_bytes())
                )[..16]
            );
            if records.iter().any(|a| a.id == id) {
                return Err(
                    "This item is already installed. Uninstall its previous local copy first."
                        .into(),
                );
            }
            let dir = data().join("apps").join(&id);
            if dir.exists() {
                return Err("An incomplete copy exists. Remove it from ~/.local/share/dropdock/apps before retrying.".into());
            }
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let result = (|| {
                let dest = if p.path.is_dir() {
                    copy_tree(&p.path, &dir)?;
                    dir.join(selected.strip_prefix(&p.path).map_err(|e| e.to_string())?)
                } else {
                    let dst = dir.join(p.path.file_name().unwrap());
                    fs::copy(&p.path, &dst).map_err(|e| e.to_string())?;
                    dst
                };
                fs::set_permissions(&dest, fs::Permissions::from_mode(0o755))
                    .map_err(|e| e.to_string())?;
                let desktop = home()
                    .join(".local/share/applications")
                    .join(format!("dropdock-{id}.desktop"));
                fs::create_dir_all(desktop.parent().unwrap()).map_err(|e| e.to_string())?;
                let name = p.name.replace(['\n', '\r'], " ");
                fs::write(&desktop,format!("[Desktop Entry]\nType=Application\nName={name}\nExec={}\nPath={}\nIcon=application-x-executable\nTerminal={terminal}\nCategories=Utility;\nX-Dropdock-Managed=true\n",exec_quote(&dest)?,dest.parent().unwrap().display())).map_err(|e|e.to_string())?;
                records.push(Local {
                    id: id.clone(),
                    name,
                    directory: dir.clone(),
                    desktop: desktop.clone(),
                });
                if let Err(e) = save(&records) {
                    fs::remove_file(desktop).ok();
                    return Err(e);
                }
                Ok(())
            })();
            if result.is_err() {
                fs::remove_dir_all(&dir).ok();
            }
            result
        }
        _ => Err("Prepare this package before installing it.".into()),
    }
}
pub fn protected(a: &App) -> bool {
    a.source == Source::Pacman
        && (matches!(
            a.id.as_str(),
            "pacman"
                | "glibc"
                | "systemd"
                | "systemd-libs"
                | "filesystem"
                | "bash"
                | "plasma-workspace"
                | "kwin"
                | "polkit"
                | "dbus"
                | "sudo"
                | "sddm"
        ) || a.id.starts_with("linux-"))
}
pub fn uninstall(a: App, tx: &Sender<Event>) -> Result<()> {
    if protected(&a) {
        return Err("This is a protected system component, not an ordinary application.".into());
    }
    match a.source {
        Source::Pacman => command("/usr/bin/pkexec", &strings(&["/usr/bin/pacman", "-R", "--noconfirm", "--", &a.id]), None, tx),
        Source::FlatpakUser | Source::FlatpakSystem => command("/usr/bin/flatpak", &strings(&["uninstall", if a.source == Source::FlatpakUser { "--user" } else { "--system" }, "--noninteractive", &a.id]), None, tx),
        Source::Snap => command("/usr/bin/pkexec", &strings(&["/usr/bin/snap", "remove", &a.id]), None, tx),
        Source::Steam => {
            if a.id.is_empty() || !a.id.chars().all(|c| c.is_ascii_digit()) {
                return Err("Invalid Steam app ID".into());
            }
            command("/usr/bin/xdg-open", &[format!("steam://uninstall/{}", a.id)], None, tx)
        }
        Source::Local => {
            let mut records = state()?;
            let index = records.iter().position(|x| x.id == a.id).ok_or("Local installation record is missing")?;
            let record = &records[index];
            if !record.id.starts_with("app-") || !record.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                return Err("Invalid installation ID".into());
            }
            let expected = data().join("apps").join(&record.id);
            let expected_desktop = home().join(".local/share/applications").join(format!("dropdock-{}.desktop", record.id));
            if record.directory != expected || record.desktop != expected_desktop {
                return Err("Invalid local installation paths; refusing removal.".into());
            }
            if expected.exists() {
                fs::remove_dir_all(expected).map_err(|e| e.to_string())?;
            }
            if record.desktop.exists() {
                fs::remove_file(&record.desktop).map_err(|e| e.to_string())?;
            }
            records.remove(index);
            save(&records)
        }
        Source::Shortcut => Err("This launcher is not owned by a supported package manager. Use the app's original installer.".into()),
    }
}
pub fn removal_preview(a: &App) -> Result<String> {
    match a.source {
        Source::Pacman => {
            let details = output("/usr/bin/pacman", &["-Qi", "--", &a.id])?;
            Ok(format!("Remove package {} only. All applications included in this package will be removed. Personal data is retained. Dependencies are NOT recursively removed; pacman refuses removal if another package still requires it.\n\n{}", a.id, details))
        }
        Source::FlatpakUser | Source::FlatpakSystem => Ok(format!("Remove {} from {}. App data is retained. Shared runtimes are not removed.", a.name, a.source.label())),
        Source::Snap => Ok(format!("Remove {} through Snap. Snap's normal snapshot behavior applies; Dropdock does not request purge.", a.name)),
        Source::Steam => Ok(format!("Open Steam's uninstall dialog for {}. Confirm the removal in Steam, then refresh this library. Dropdock will not report it as removed until you finish there.", a.name)),
        Source::Local => Ok("Delete Dropdock's private application copy and its menu launcher. The original source and personal configuration remain untouched.".into()),
        Source::Shortcut => Err("Unmanaged launcher: removal is unavailable.".into()),
    }
}
pub fn arch_preview(p: &Package) -> Result<String> {
    check(p)?;
    output(
        "/usr/bin/pacman",
        &[
            "-Up",
            "--print-format",
            "%n %v (%s bytes)",
            "--",
            p.path.to_str().unwrap(),
        ],
    )
}
pub fn repository_search(query: &str) -> Result<String> {
    require_arch()?;
    let query = query.trim();
    if query.is_empty() || query.starts_with('-') {
        return Err("Enter an application name.".into());
    }
    let result = Command::new("/usr/bin/pacman")
        .args(["-Ss", "--", query])
        .env("LC_ALL", "C")
        .output()
        .map_err(|e| e.to_string())?;
    if result.status.success() || (result.status.code() == Some(1) && result.stderr.is_empty()) {
        Ok(String::from_utf8_lossy(&result.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&result.stderr).trim().into())
    }
}
pub fn install_repository(name: &str, tx: &Sender<Event>) -> Result<()> {
    require_arch()?;
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "@._+-/".contains(c))
        || name.starts_with('-')
    {
        return Err("Enter an exact repository package name.".into());
    }
    command(
        "/usr/bin/konsole",
        &strings(&[
            "--separate",
            "--nofork",
            "-e",
            "/usr/bin/pkexec",
            "/usr/bin/pacman",
            "-S",
            "--",
            name,
        ]),
        None,
        tx,
    )
}
pub fn refresh_debtap(tx: &Sender<Event>) -> Result<()> {
    require_arch()?;
    command(
        "/usr/bin/pkexec",
        &strings(&["/usr/bin/debtap", "-u"]),
        None,
        tx,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_install_and_remove_preserve_original() {
        let dir = workspace().unwrap();
        let src = dir.join("test app %f.AppImage");
        fs::write(&src, b"\x7fELF\0\0\0\0AI\x02dummy").unwrap();
        let p = inspect(src.clone()).unwrap();
        let (tx, _rx) = std::sync::mpsc::channel();
        install(p, None, false, &tx).unwrap();
        let record = state()
            .unwrap()
            .into_iter()
            .find(|r| r.name == "test app %f.AppImage")
            .unwrap();
        let desktop = fs::read_to_string(&record.desktop).unwrap();
        assert!(desktop.contains("%%f"));
        assert!(record.directory.exists());
        uninstall(
            App {
                id: record.id,
                name: record.name,
                version: String::new(),
                source: Source::Local,
                desktop: Some(record.desktop.clone()),
                is_app: true,
                description: String::new(),
            },
            &tx,
        )
        .unwrap();
        assert!(src.exists());
        assert!(!record.directory.exists());
        assert!(!record.desktop.exists());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn portable_copy_preserves_runtime_and_rejects_recursion() {
        let dir = workspace().unwrap();
        let src = dir.join("src");
        fs::create_dir_all(src.join("node_modules/runtime")).unwrap();
        fs::write(src.join("node_modules/runtime/a"), "data").unwrap();
        assert!(copy_tree(&src, &src.join("nested")).is_err());
        copy_tree(&src, &dir.join("copy")).unwrap();
        assert!(dir.join("copy/node_modules/runtime/a").exists());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn ansi_log_cleanup() {
        assert_eq!(clean_log("\x1b[1;32mhello\x1b[0m"), "hello");
    }
    #[test]
    fn extensions() {
        assert_eq!(kind(Path::new("hello.pkg.tar.zst")), Kind::Arch);
        assert_eq!(kind(Path::new("hello.DEB")), Kind::Deb);
        assert_eq!(kind(Path::new("a.rpm")), Kind::Rpm);
        assert_eq!(kind(Path::new("x.exe")), Kind::Unknown);
    }
    #[test]
    fn desktop_sections() {
        assert_eq!(
            field(
                "[Desktop Entry]\nName=Main\n[Desktop Action X]\nName=Other",
                "Name"
            ),
            "Main"
        );
    }
    #[test]
    fn desktop_exec_escaping() {
        let q = exec_quote(Path::new("/tmp/a \"b\" $x %f")).unwrap();
        assert_eq!(q, "\"/tmp/a \\\"b\\\" \\$x %%f\"");
        assert!(exec_quote(Path::new("/tmp/a\nb")).is_err());
    }
    #[test]
    fn pacman_description() {
        assert_eq!(desc("%NAME%\nfoo\n\n%VERSION%\n1.2\n\n", "VERSION"), "1.2");
    }
    #[test]
    fn changed_file_rejected() {
        let dir = workspace().unwrap();
        let p = dir.join("a.txt");
        fs::write(&p, "old").unwrap();
        let item = inspect(p.clone()).unwrap();
        fs::write(&p, "new").unwrap();
        assert!(check(&item).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn external_symlinks_rejected() {
        let dir = workspace().unwrap();
        let source = dir.join("source");
        fs::create_dir(&source).unwrap();
        std::os::unix::fs::symlink("/etc/passwd", source.join("escape")).unwrap();
        assert!(copy_tree(&source, &dir.join("dest")).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod dependency_failure_tests {
    use super::*;
    #[test]
    fn dependency_block_is_distinct_from_partial_failure() {
        let lines = strings(&[
            ":: removing ripgrep breaks dependency 'ripgrep' required by hwdetect",
            "error: failed to prepare transaction (could not satisfy dependencies)",
        ]);
        let args = strings(&["/usr/bin/pacman", "-R", "--noconfirm", "--", "ripgrep"]);
        let message = dependency_failure("/usr/bin/pkexec", &args, &lines).unwrap();
        assert!(message.contains("No packages were removed"));
        assert!(message.contains("hwdetect"));
        assert!(dependency_failure("/usr/bin/other", &args, &lines).is_none());
        assert!(dependency_failure("/usr/bin/pkexec", &args, &lines[..1]).is_none());
        assert!(dependency_failure("/usr/bin/pacman", &strings(&["-U"]), &lines).is_none());
    }
}
