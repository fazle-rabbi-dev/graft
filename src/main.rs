use anyhow::{anyhow, Context, Result};
use clap::Parser;
use flate2::read::GzDecoder;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs::{self, File};
use std::io::{copy, Write};
use std::path::PathBuf;
use tar::Archive;

#[derive(Parser, Debug)]
#[command(name = "graft", about = "Download a subfolder of a GitHub repo")]
struct Cli {
    /// Repository in form owner/repo
    repo: Option<String>,
    /// Subfolder path inside the repo (e.g. src/components)
    subpath: Option<String>,
    /// Destination directory (created if missing)
    dest: Option<PathBuf>,
    /// Git ref (branch, tag, or commit sha). Defaults to HEAD.
    #[arg(long, default_value = "HEAD")]
    ref_: String,
    /// GitHub token (also reads GITHUB_TOKEN env var)
    #[arg(long, env = "GITHUB_TOKEN")]
    token: Option<String>,
    /// List top-level entries of the subpath instead of downloading
    #[arg(long)]
    list: bool,
}

fn main() -> Result<()> {
    let mut cli = Cli::parse();
    if cli.repo.is_none() || cli.subpath.is_none() {
        // interactive mode (double-click / no args)
        println!("=== graft: download a subfolder of a GitHub repo ===");
        let mut buf = String::new();
        if cli.repo.is_none() {
            print!("Repository (owner/repo): ");
            let _ = std::io::stdout().flush();
            buf.clear();
            std::io::stdin().read_line(&mut buf)?;
            cli.repo = Some(buf.trim().to_string());
        }
        if cli.subpath.is_none() {
            print!("Subfolder path (e.g. src/components): ");
            let _ = std::io::stdout().flush();
            buf.clear();
            std::io::stdin().read_line(&mut buf)?;
            cli.subpath = Some(buf.trim().to_string());
        }
        if cli.dest.is_none() {
            print!("Destination directory (blank = repo name): ");
            let _ = std::io::stdout().flush();
            buf.clear();
            std::io::stdin().read_line(&mut buf)?;
            let v = buf.trim();
            if !v.is_empty() { cli.dest = Some(PathBuf::from(v)); }
        }
        if cli.ref_ == "HEAD" {
            print!("Git ref (blank = HEAD): ");
            let _ = std::io::stdout().flush();
            buf.clear();
            std::io::stdin().read_line(&mut buf)?;
            let v = buf.trim();
            if !v.is_empty() { cli.ref_ = v.to_string(); }
        }
    }
    let repo = cli.repo.clone().unwrap();
    let subpath = cli.subpath.clone().unwrap();
    cli.subpath = Some(subpath.clone());
    cli.repo = Some(repo.clone());
    let (owner, name) = parse_repo(&repo)?;
    let url = format!(
        "https://codeload.github.com/{}/{}/tar.gz/{}",
        owner, name, encode_ref(&cli.ref_)
    );

    let mut req = ureq::get(&url);
    if let Some(t) = &cli.token {
        req = req.header("Authorization", &format!("token {}", t));
    }
    let resp = req.call().context("failed to download tarball")?;
    if resp.status() != 200 {
        return Err(anyhow!("HTTP {} from {}", resp.status(), url));
    }

    let total = resp.headers().get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let pb = ProgressBar::new(total);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
        .unwrap()
        .progress_chars("#>-"));
    let reader = pb.wrap_read(resp.into_body().into_reader());

    let gz = GzDecoder::new(reader);
    let mut archive = Archive::new(gz);

    let dest = cli.dest.clone().unwrap_or_else(|| {
        PathBuf::from(name.clone())
    });
    fs::create_dir_all(&dest)?;

    let mut count = 0u64;
    let mut listed: Vec<String> = Vec::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_path_buf();
        // strip root dir component
        let mut comps: Vec<_> = path.components().collect();
        if comps.is_empty() { continue; }
        comps.remove(0);
        let rel = comps.iter().map(|c| c.as_os_str()).collect::<PathBuf>();
        if rel.as_os_str().is_empty() { continue; }

        // does it live under subpath?
        let sub = PathBuf::from(&subpath);
        let after = match strip_prefix(&rel, &sub) {
            Some(a) => a,
            None => continue,
        };

        if cli.list {
            if after.as_os_str().is_empty() {
                listed.push(rel.display().to_string());
            }
            continue;
        }

        let out = dest.join(&after);
        if entry.header().entry_type().is_dir() {
            fs::create_dir_all(&out)?;
        } else if entry.header().entry_type().is_file() {
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut f = File::create(&out)?;
            copy(&mut entry, &mut f)?;
            // preserve mode on unix
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(mode) = entry.header().mode() {
                    let _ = fs::set_permissions(&out, fs::Permissions::from_mode(mode));
                }
            }
            count += 1;
        }
        // skip symlinks for safety
    }
    pb.finish_with_message("done");

    if cli.list {
        listed.sort();
        listed.dedup();
        for l in &listed {
            println!("{}", l);
        }
        if listed.is_empty() {
            println!("(nothing found under '{}')", subpath);
        }
    } else {
        println!("grafted {} files into {}", count, dest.display());
    }
    Ok(())
}

fn parse_repo(s: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(anyhow!("repo must be owner/repo"));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

fn encode_ref(r: &str) -> String {
    // allow slash refs like refs/heads/main passed directly
    if r.starts_with("refs/") {
        r.to_string()
    } else if r == "HEAD" {
        "HEAD".to_string()
    } else {
        // treat as branch name; codeload accepts branch names in path
        r.to_string()
    }
}

fn strip_prefix(rel: &PathBuf, sub: &PathBuf) -> Option<PathBuf> {
    // returns path after sub; None if rel not under sub
    let rel_comps: Vec<_> = rel.components().collect();
    let sub_comps: Vec<_> = sub.components().collect();
    if rel_comps.len() < sub_comps.len() { return None; }
    for (a, b) in rel_comps.iter().zip(sub_comps.iter()) {
        if a != b { return None; }
    }
    let mut out = PathBuf::new();
    for c in &rel_comps[sub_comps.len()..] {
        out.push(c);
    }
    Some(out)
}
