//! A simple example of `#[derive(Quiver)]`, mixing strongly-typed quiver columns
//! with dynamically-typed raw arrow columns.
//!
//! Run with: `cargo run --example example`

#![expect(clippy::print_stdout)] // This is an example, after all

use std::collections::BTreeMap;
use std::sync::Arc;

use arrow_quiver::arrow::array::{ArrayRef, StringArray};
use arrow_quiver::{Column, Quiver};

/// A set of measurements.
#[derive(Quiver)]
struct Measurements {
    /// Metadata of the record batch.
    #[quiver(metadata)]
    metadata: BTreeMap<String, String>,

    /// Name of the sensor.
    ///
    /// A strongly-typed quiver column: guaranteed to be `Utf8` with no nulls.
    sensor: Column<String>,

    /// Measured temperature.
    ///
    /// `Column<Option<f64>>`: the *values* may be null.
    /// (`Option<Column<f64>>` would instead mean the whole *column* may be missing.)
    temperature: Column<Option<f64>>,

    /// A raw arrow array: any datatype, any nullability.
    ///
    /// Use raw arrow types when you *want* things to be dynamic.
    comment: ArrayRef,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Build a typed record — the struct literal is the builder:
    let measurements = Measurements {
        metadata: BTreeMap::from([("origin".to_owned(), "lab".to_owned())]),
        sensor: Column::from_values(["kitchen", "bedroom", "attic"]),
        temperature: Column::from_values([Some(22.1), None, Some(30.5)]),
        comment: Arc::new(StringArray::from(vec!["cozy", "quiet", "spooky"])),
    };

    // Convert into an ordinary arrow `RecordBatch` (fails on column length mismatch):
    let batch = measurements.into_record_batch()?;

    // … write to disk, send over the network, etc …

    // Parse it back: validates the schema, then downcasts the columns (zero-copy):
    let measurements = Measurements::try_from(batch)?;

    println!("origin: {:?}", measurements.metadata.get("origin"));

    // Typed columns iterate without any downcasting or unwrapping:
    for (sensor, temperature) in
        std::iter::zip(measurements.sensor.iter(), measurements.temperature.iter())
    {
        match temperature {
            Some(temperature) => println!("{sensor}: {temperature} °C"),
            None => println!("{sensor}: no reading"),
        }
    }

    // The raw arrow column is dynamically typed:
    println!("comments: {:?}", measurements.comment);

    Ok(())
}
