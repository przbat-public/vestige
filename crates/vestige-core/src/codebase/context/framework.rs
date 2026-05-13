//! Recognised frameworks (Rust/JS/Python/JVM/etc.).

use serde::{Deserialize, Serialize};

/// Known frameworks that can be detected
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Framework {
    // Rust
    Tauri,
    Actix,
    Axum,
    Rocket,
    Tokio,
    Diesel,
    SeaOrm,

    // JavaScript/TypeScript
    React,
    Vue,
    Angular,
    Svelte,
    NextJs,
    NuxtJs,
    Express,
    NestJs,
    Deno,
    Bun,

    // Python
    Django,
    Flask,
    FastApi,
    Pytest,
    Poetry,

    // Other
    Spring,  // Java
    Rails,   // Ruby
    Laravel, // PHP
    DotNet,  // C#

    Other(String),
}

impl Framework {
    pub fn name(&self) -> &str {
        match self {
            Self::Tauri => "Tauri",
            Self::Actix => "Actix",
            Self::Axum => "Axum",
            Self::Rocket => "Rocket",
            Self::Tokio => "Tokio",
            Self::Diesel => "Diesel",
            Self::SeaOrm => "SeaORM",
            Self::React => "React",
            Self::Vue => "Vue",
            Self::Angular => "Angular",
            Self::Svelte => "Svelte",
            Self::NextJs => "Next.js",
            Self::NuxtJs => "Nuxt.js",
            Self::Express => "Express",
            Self::NestJs => "NestJS",
            Self::Deno => "Deno",
            Self::Bun => "Bun",
            Self::Django => "Django",
            Self::Flask => "Flask",
            Self::FastApi => "FastAPI",
            Self::Pytest => "Pytest",
            Self::Poetry => "Poetry",
            Self::Spring => "Spring",
            Self::Rails => "Rails",
            Self::Laravel => "Laravel",
            Self::DotNet => ".NET",
            Self::Other(name) => name,
        }
    }
}
