use askama::Template;

#[derive(Template)]
#[template(path = "mirror.html")]
pub struct MirrorTemplate {
    pub srcuri_url: String,
    pub repo_name: String,
    pub file_path: String,
    pub line: String,
    pub og_description: String,
    pub view_url: String,
    pub provider_name: String,
}

#[derive(Template)]
#[template(path = "error.html")]
pub struct ErrorTemplate {
    pub message: String,
    pub url: String,
}
