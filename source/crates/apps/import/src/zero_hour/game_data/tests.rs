use super::*;

#[test]
fn the_terrain_draws_three_way_blends_for_any_count_but_zero() {
    let mut data = GameDataIni::default();
    let terrain = |data: &GameDataIni| GameData::of(data);
    assert_eq!(
        terrain(&data),
        GameData {
            adjust_cliff_textures: false,
            three_way_blends: true,
        }
    );
    for (count, blends) in [(0, false), (2, true)] {
        data.three_way_blends = count;
        data.adjust_cliff_textures = true;
        assert_eq!(
            terrain(&data),
            GameData {
                adjust_cliff_textures: true,
                three_way_blends: blends,
            }
        );
    }
}
