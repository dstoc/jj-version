use proc_macro::{Literal, TokenStream, TokenTree};
use proc_macro2::TokenStream as TokenStream2;
use std::process::Command;
use syn::parse::{Parse, ParseStream};
use syn::{Expr, Ident, Result, Token};

const JJ_BASE_ARGS: [&str; 4] = [
    "--ignore-working-copy",
    "--at-op=@",
    "--no-pager",
    "--color=never",
];

/// Embed a Jujutsu-derived version string at compile time.
///
/// The following named argument is required:
///
/// - `fallback`:
///   A Rust expression to emit if `jj` is unavailable, the current directory is
///   not a Jujutsu repository, or version resolution fails. The expression must
///   evaluate to `&'static str` in the calling context.
///
/// If Jujutsu metadata is available, the macro expands to a string literal
/// similar to `git describe --tags --always`, for example `v1.2.3`,
/// `v1.2.3-4-gabc123def456`, or `abc123def456`.
///
/// The macro invokes the `jj` binary directly with `--ignore-working-copy` and
/// never snapshots or mutates the repository. It does not emit a dirty suffix.
///
/// # Examples
///
/// ```ignore
/// # use jj_version::jj_version;
/// const VERSION: &str = jj_version!(
///     fallback = env!("CARGO_PKG_VERSION"),
/// );
/// ```
///
/// ```ignore
/// # use jj_version::jj_version;
/// const VERSION: &str = jj_version!(
///     fallback = concat!("fallback-", env!("CARGO_PKG_VERSION")),
/// );
/// ```
///
/// ```ignore
/// # use jj_version::jj_version;
/// const VERSION: &str = jj_version!(
///     fallback = git_version::git_version!(
///         args = ["--tags", "--dirty", "--always", "--abbrev=12"],
///         fallback = env!("CARGO_PKG_VERSION"),
///     ),
/// );
/// ```
#[proc_macro]
pub fn jj_version(input: TokenStream) -> TokenStream {
    let parsed = match syn::parse::<VersionArgs>(input.clone()) {
        Ok(args) => args,
        Err(err) => return compile_error(err.to_string()),
    };

    match resolve_jj_version() {
        Some(version) => TokenStream::from(TokenTree::Literal(Literal::string(&version))),
        None => TokenStream::from(parsed.fallback),
    }
}

struct VersionArgs {
    fallback: TokenStream2,
}

impl Parse for VersionArgs {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        if input.is_empty() {
            return Err(input.error("expected `fallback = <expr>`"));
        }

        let ident: Ident = input.parse()?;
        if ident != "fallback" {
            return Err(syn::Error::new(ident.span(), "expected `fallback`"));
        }

        input.parse::<Token![=]>()?;

        let mut fallback_tokens: Vec<proc_macro2::TokenTree> =
            input.parse::<TokenStream2>()?.into_iter().collect();
        if fallback_tokens.is_empty() {
            return Err(input.error("expected `fallback = <expr>`"));
        }

        if matches!(fallback_tokens.last(), Some(proc_macro2::TokenTree::Punct(p)) if p.as_char() == ',')
        {
            fallback_tokens.pop();
        }

        let fallback = TokenStream2::from_iter(fallback_tokens);
        syn::parse2::<Expr>(fallback.clone())
            .map_err(|err| syn::Error::new(err.span(), "expected `fallback = <expr>`"))?;

        Ok(VersionArgs { fallback })
    }
}

fn compile_error(message: impl AsRef<str>) -> TokenStream {
    let literal = Literal::string(message.as_ref());
    format!("compile_error!({literal})")
        .parse()
        .expect("static compile_error token stream")
}

fn resolve_jj_version() -> Option<String> {
    let effective_rev = "coalesce(@ ~ empty(), @-)";
    let tag_revset = "latest(heads(tags() & ::(coalesce(@ ~ empty(), @-))))";

    let short = run_jj_single_line(&[
        "log",
        "-G",
        "-r",
        effective_rev,
        "-T",
        r#"commit_id.short(12) ++ "\n""#,
    ])?;

    let tag = match run_jj_first_non_empty_line(&[
        "tag",
        "list",
        "-r",
        tag_revset,
        "--sort",
        "name",
        "-T",
        r#"name ++ "\n""#,
    ]) {
        Some(tag) => tag,
        None => return Some(short),
    };

    let count_revset = format!("({tag_revset})..({effective_rev})");
    let count = run_jj_single_line(&["log", "--count", "-r", &count_revset])?
        .parse::<usize>()
        .ok()?;

    if count == 0 {
        Some(tag)
    } else {
        Some(format!("{tag}-{count}-g{short}"))
    }
}

fn run_jj_single_line(args: &[&str]) -> Option<String> {
    let stdout = run_jj_output(args)?;
    let mut lines = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let line = lines.next()?;
    if lines.next().is_some() {
        return None;
    }

    Some(line.to_owned())
}

fn run_jj_first_non_empty_line(args: &[&str]) -> Option<String> {
    let stdout = run_jj_output(args)?;
    stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

fn run_jj_output(args: &[&str]) -> Option<String> {
    let output = Command::new("jj")
        .args(JJ_BASE_ARGS)
        .args(args)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8(output.stdout).ok()
}
