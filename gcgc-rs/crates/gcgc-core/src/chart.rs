//! Renderer-agnostic chart description.
//!
//! The ported plotting functions record the same calls the original made on a matplotlib `Axes`
//! (`plot`, `scatter`, `bar`, `imshow`, `text`) plus the axis decorations. A front end (Plotly in the
//! web app) turns these into pixels; tests compare them with the calls recorded from the Python code.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Color {
    /// RGB components in 0..=1, as matplotlib tuples.
    Rgb([f64; 3]),
    /// A named / CSS color such as `"blue"`.
    Named(String),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "method", rename_all = "lowercase")]
pub enum Call {
    /// `ax.plot(x, y, ...)`; with `marker` and an empty `linestyle` it is a scatter of markers.
    Plot {
        x: Vec<Option<f64>>,
        y: Vec<Option<f64>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        color: Option<Color>,
        #[serde(skip_serializing_if = "Option::is_none")]
        alpha: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        marker: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        linestyle: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        markersize: Option<f64>,
    },
    /// `ax.scatter(x, y, ...)`
    Scatter {
        x: Vec<Option<f64>>,
        y: Vec<Option<f64>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        color: Option<Color>,
        #[serde(skip_serializing_if = "Option::is_none")]
        alpha: Option<f64>,
    },
    /// `ax.bar(x, height, ...)`
    Bar {
        x: f64,
        height: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        width: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        color: Option<Color>,
        #[serde(skip_serializing_if = "Option::is_none")]
        alpha: Option<f64>,
    },
    /// `ax.imshow(data, vmin=..., cmap=...)`: rows are drawn top to bottom.
    Imshow { data: Vec<Vec<f64>>, vmin: f64, cmap: String },
    /// `ax.text(x, y, s, color=...)`, centred.
    Text { x: f64, y: f64, text: String, color: String },
}

impl Call {
    pub fn label(&self) -> Option<&str> {
        match self {
            Call::Plot { label, .. } | Call::Scatter { label, .. } | Call::Bar { label, .. } => label.as_deref(),
            _ => None,
        }
    }

    pub fn method(&self) -> &'static str {
        match self {
            Call::Plot { .. } => "plot",
            Call::Scatter { .. } => "scatter",
            Call::Bar { .. } => "bar",
            Call::Imshow { .. } => "imshow",
            Call::Text { .. } => "text",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Scale {
    #[default]
    Linear,
    Log,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct Axes {
    pub title: String,
    pub xlabel: String,
    pub ylabel: String,
    pub yscale: Scale,
    /// Explicit tick positions/labels (otherwise automatic).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xticks: Option<Vec<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xticklabels: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yticks: Option<Vec<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yticklabels: Option<Vec<String>>,
    /// Tick label rotation in degrees (matplotlib convention: positive is counter-clockwise).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xtick_rotation: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grid_alpha: Option<f64>,
    /// Whether `legend()` was called.
    pub legend: bool,
    /// Colorbar label for heatmaps.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colorbar: Option<String>,
    pub calls: Vec<Call>,
}

impl Axes {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, call: Call) {
        self.calls.push(call);
    }

    /// Legend entries in matplotlib order: labelled lines/collections in creation order, then bars.
    pub fn legend_labels(&self) -> Vec<String> {
        let visible = |c: &&Call| c.label().is_some_and(|l| !l.starts_with('_'));
        let non_bar = self.calls.iter().filter(|c| !matches!(c, Call::Bar { .. })).filter(visible);
        let bar = self.calls.iter().filter(|c| matches!(c, Call::Bar { .. })).filter(visible);
        non_bar.chain(bar).filter_map(|c| c.label().map(str::to_string)).collect()
    }
}

/// A figure holds one main axes (matplotlib's colorbar axes are folded into `Axes::colorbar`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Figure {
    pub axes: Axes,
    /// Legend entries, precomputed for renderers.
    pub legend_labels: Vec<String>,
}

impl Figure {
    pub fn new(axes: Axes) -> Self {
        let legend_labels = if axes.legend { axes.legend_labels() } else { Vec::new() };
        Self { axes, legend_labels }
    }
}

/// The 21-colour cycle of `get_colors_and_alphas` (https://sashamaps.net/docs/resources/20-colors/).
pub const PRESET_COLORS: [(u8, u8, u8); 21] = [
    (230, 25, 75),
    (60, 180, 75),
    (215, 215, 25),
    (0, 130, 200),
    (245, 130, 48),
    (145, 30, 180),
    (70, 240, 240),
    (240, 50, 230),
    (210, 245, 60),
    (250, 190, 212),
    (0, 128, 128),
    (220, 190, 255),
    (170, 110, 40),
    (255, 250, 200),
    (128, 0, 0),
    (170, 255, 195),
    (128, 128, 0),
    (255, 215, 180),
    (0, 0, 128),
    (128, 128, 128),
    (0, 0, 0),
];

/// `get_colors_and_alphas(n)`: the first `n` colours of the repeating preset cycle, alpha 1.
pub fn colors_and_alphas(n: usize) -> (Vec<Color>, Vec<f64>) {
    let colors = (0..n)
        .map(|i| {
            let (r, g, b) = PRESET_COLORS[i % PRESET_COLORS.len()];
            Color::Rgb([r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0])
        })
        .collect();
    (colors, vec![1.0; n])
}
