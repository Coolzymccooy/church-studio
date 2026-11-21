//! The desktop state is an additive wire contract. Optional mixer banks
//! contribute tablet faders when the corresponding state fields are present.
use super::{Status, Target, ViewControl};
use serde::{Deserialize, Serialize};

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct ExtraFaders {
    fx_returns: Vec<ReturnFader>,
    groups: Option<DcaFaders>,
}

#[derive(Deserialize)]
struct ReturnFader {
    id: String,
    fader_db: f32,
    mute: bool,
}

#[derive(Deserialize)]
struct DcaFaders {
    faders: [f32; 8],
    dca_muted: u8,
}

pub(super) fn read(state: &impl Serialize) -> Result<Vec<ViewControl>, Status> {
    let value = serde_json::to_value(state).map_err(|_| Status::INTERNAL_SERVER_ERROR)?;
    let faders: ExtraFaders =
        serde_json::from_value(value).map_err(|_| Status::INTERNAL_SERVER_ERROR)?;
    let mut controls = Vec::new();
    for ret in faders.fx_returns {
        let name = match ret.id.as_str() {
            "fx_a" => "FX A",
            "fx_b" => "FX B",
            _ => continue,
        };
        controls.push(ViewControl {
            target: Target::Return { id: ret.id },
            name: name.into(),
            db: ret.fader_db,
            muted: Some(ret.mute),
        });
    }
    if let Some(groups) = faders.groups {
        controls.extend(
            groups
                .faders
                .into_iter()
                .enumerate()
                .map(|(index, db)| ViewControl {
                    target: Target::Dca {
                        index: index as u32,
                    },
                    name: format!("DCA {}", index + 1),
                    db,
                    muted: Some(groups.dca_muted & (1 << index) != 0),
                }),
        );
    }
    Ok(controls)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_available_return_and_group_controls_from_desk_state() {
        let state = serde_json::json!({
            "groups": { "faders": [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -8.0], "dca_muted": 128 },
            "fxReturns": [{ "id": "fx_b", "fader_db": -15.0, "mute": false }],
        });
        assert!(read(&serde_json::json!({})).unwrap().is_empty());
        let controls = read(&state).unwrap();
        assert_eq!(controls.len(), 9);
        assert_eq!(controls[0].name, "FX B");
        assert_eq!(controls[8].db, -8.0);
        assert_eq!(controls[8].muted, Some(true));
    }
}
