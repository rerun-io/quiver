//! A simple example of `#[derive(Quiver)]`.
//!
//! Run with: `cargo run --example example`

#![expect(clippy::print_stdout)] // This is an example, after all

use std::collections::BTreeMap;

use arrow_quiver::Quiver;
use arrow_quiver::arrow::array::{Float64Array, StringArray};
use arrow_quiver::arrow::record_batch::RecordBatch;

/// A set of measurements.
#[derive(Quiver)]
struct Measurements {
    /// Metadata of the record batch.
    #[quiver(metadata)]
    metadata: BTreeMap<String, String>,

    /// Name of the sensor. May not contain nulls.
    #[quiver(non_null)]
    sensor: StringArray,

    /// Measured temperature. The whole column may be missing.
    temperature: Option<Float64Array>,
}

fn main() -> Result<(), arrow_quiver::Error> {
    // Build a typed record — the struct literal is the builder:
    let measurements = Measurements {
        metadata: BTreeMap::from([("origin".to_owned(), "lab".to_owned())]),
        sensor: StringArray::from(vec!["kitchen", "bedroom", "attic"]),
        temperature: Some(Float64Array::from(vec![22.1, 21.9, 30.5])),
    };

    // Convert into an ordinary arrow `RecordBatch` (fails on column length mismatch):
    let batch = RecordBatch::try_from(measurements)?;

    // … write to disk, send over the network, etc …

    // Parse it back: validates the schema, then downcasts the columns (zero-copy):
    let measurements = Measurements::try_from(batch)?;

    println!("origin: {:?}", measurements.metadata.get("origin"));

    if let Some(temperature) = &measurements.temperature {
        for (sensor, temperature) in std::iter::zip(measurements.sensor.iter(), temperature.iter())
        {
            if let (Some(sensor), Some(temperature)) = (sensor, temperature) {
                println!("{sensor}: {temperature} °C");
            }
        }
    }

    Ok(())
}
