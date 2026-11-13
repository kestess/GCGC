//! gcgc-core: the analysis engine of GCGC (Garbage Collection Graph Collector), ported from the
//! Python/Jupyter tool. It parses JVM unified GC logs (`-Xlog:gc*`) into event tables and produces
//! renderer-agnostic chart descriptions and text tables.

pub mod analyses;
pub mod chart;
pub mod csv;
pub mod npsort;
pub mod out;
pub mod parse;
pub mod plotting;
pub mod pyfmt;
pub mod query;
pub mod run;
pub mod stats;
pub mod table;

pub use out::Out;
pub use run::{build_run, LogFile, Run, RunOptions};
pub use table::{Cell, Column, Table};
