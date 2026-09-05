use serde::de::Error;
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::Cell;

#[derive(Serialize)]
struct Runs<'a> {
    runs: Vec<(u16, &'a Cell)>,
}

pub(crate) fn serialize<S: Serializer>(
    rows: &[Vec<Cell>],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut sequence = serializer.serialize_seq(Some(rows.len()))?;
    for row in rows {
        let mut runs: Vec<(u16, &Cell)> = Vec::new();
        for cell in row {
            if let Some((count, previous)) = runs.last_mut()
                && *previous == cell
                && *count < u16::MAX
            {
                *count += 1;
            } else {
                runs.push((1, cell));
            }
        }
        if runs.len() * 2 < row.len() {
            sequence.serialize_element(&Runs { runs })?;
        } else {
            sequence.serialize_element(row)?;
        }
    }
    sequence.end()
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Row {
    Runs { runs: Vec<(u16, Cell)> },
    Cells(Vec<Cell>),
}

pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Vec<Cell>>, D::Error> {
    let rows = Vec::<Row>::deserialize(deserializer)?;
    let mut remaining: usize = 16 * 1024 * 1024 / std::mem::size_of::<Cell>();
    rows.into_iter()
        .map(|row| {
            let length = match &row {
                Row::Runs { runs } => runs.iter().map(|(count, _)| usize::from(*count)).sum(),
                Row::Cells(cells) => cells.len(),
            };
            remaining = remaining
                .checked_sub(length)
                .ok_or_else(|| D::Error::custom("cell frame is too large"))?;
            Ok(match row {
                Row::Cells(cells) => cells,
                Row::Runs { runs } => runs
                    .into_iter()
                    .flat_map(|(count, cell)| std::iter::repeat_n(cell, usize::from(count)))
                    .collect(),
            })
        })
        .collect()
}
