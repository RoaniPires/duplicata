use duplicata_core::filter_strip::Rect;
use duplicata_core::translucency::{
    alpha_plan, opens_any_transparency, AlphaOp, WindowRegions, OPAQUE, TRANSLUCENT_ALPHA,
};

fn r(left: i32, top: i32, right: i32, bottom: i32) -> Rect {
    Rect {
        left,
        top,
        right,
        bottom,
    }
}

fn client() -> Rect {
    r(0, 0, 504, 521)
}

fn regions() -> WindowRegions {
    WindowRegions {
        client: client(),
        banner: None,
        opaque_islands: vec![r(0, 64, 504, 92)],
    }
}

#[test]
fn the_plan_always_starts_by_making_the_whole_buffer_opaque() {
    for rg in [
        regions(),
        WindowRegions::default(),
        WindowRegions {
            client: client(),
            banner: Some(client()),
            opaque_islands: vec![],
        },
        WindowRegions {
            client: client(),
            banner: None,
            opaque_islands: vec![],
        },
    ] {
        let plan = alpha_plan(&rg);
        assert!(!plan.is_empty(), "o plano nunca pode ser vazio");
        assert_eq!(
            plan[0],
            AlphaOp {
                rect: None,
                alpha: OPAQUE
            },
            "o primeiro passo tem de ser o buffer INTEIRO opaco"
        );
    }
}

#[test]
fn truncating_the_plan_anywhere_leaves_the_window_opaque_never_invisible() {
    let plan = alpha_plan(&regions());
    let only_first = &plan[..1];
    assert!(
        !opens_any_transparency(only_first),
        "o prefixo mínimo do plano não pode abrir transparência nenhuma"
    );
}

#[test]
fn every_transparency_comes_after_the_opaque_reset() {
    let plan = alpha_plan(&regions());
    let first_translucent = plan
        .iter()
        .position(|o| o.alpha == TRANSLUCENT_ALPHA)
        .unwrap();
    assert!(
        first_translucent > 0,
        "abrir transparência antes de opacar o buffer é a regressão do R15"
    );
}

#[test]
fn every_opaque_island_comes_after_the_translucent_pass() {
    let rg = WindowRegions {
        client: client(),
        banner: None,
        opaque_islands: vec![r(0, 64, 504, 92), r(6, 200, 498, 202)],
    };
    let plan = alpha_plan(&rg);
    let translucent_at = plan
        .iter()
        .position(|o| o.alpha == TRANSLUCENT_ALPHA)
        .unwrap();
    let islands: Vec<usize> = plan
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(_, o)| o.alpha == OPAQUE)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(islands.len(), 2);
    for i in islands {
        assert!(
            i > translucent_at,
            "ilha opaca no índice {i}, antes do passo translúcido"
        );
    }
}

#[test]
fn a_banner_makes_the_whole_frame_opaque_with_no_transparency_at_all() {
    let rg = WindowRegions {
        client: client(),
        banner: Some(client()),
        opaque_islands: vec![r(0, 64, 504, 92)],
    };
    let plan = alpha_plan(&rg);
    assert_eq!(plan.len(), 1);
    assert!(!opens_any_transparency(&plan));
}

#[test]
fn islands_are_clipped_to_the_client_and_empty_ones_are_dropped() {
    let rg = WindowRegions {
        client: client(),
        banner: None,
        opaque_islands: vec![r(-50, 64, 600, 92), r(1000, 0, 1100, 10), r(10, 10, 10, 40)],
    };
    let plan = alpha_plan(&rg);
    let islands: Vec<Rect> = plan.iter().skip(2).filter_map(|o| o.rect).collect();
    assert_eq!(islands, vec![r(0, 64, 504, 92)], "só a recortada sobrevive");
}

#[test]
fn a_degenerate_client_opens_no_transparency() {
    let rg = WindowRegions {
        client: r(0, 0, 0, 0),
        banner: None,
        opaque_islands: vec![r(0, 0, 10, 10)],
    };
    let plan = alpha_plan(&rg);
    assert_eq!(plan.len(), 1);
    assert!(!opens_any_transparency(&plan));
}

#[test]
fn with_no_islands_the_client_is_translucent_and_nothing_else_happens() {
    let rg = WindowRegions {
        client: client(),
        banner: None,
        opaque_islands: vec![],
    };
    let plan = alpha_plan(&rg);
    assert_eq!(plan.len(), 2);
    assert_eq!(plan[1].alpha, TRANSLUCENT_ALPHA);
    assert_eq!(plan[1].rect, Some(client()));
}

#[test]
fn the_translucent_alpha_is_partial_not_zero() {
    const {
        assert!(
            TRANSLUCENT_ALPHA > 0,
            "alfa 0 torna o contraste inverificável"
        );
        assert!(
            TRANSLUCENT_ALPHA < OPAQUE,
            "sem translucidez nenhuma, não há FR-001a"
        );
        assert!(
            TRANSLUCENT_ALPHA >= 200,
            "com menos de ~78% de superfície o pior caso de contraste de text_dim não fecha 4,5:1 em nenhum tom discreto"
        );
    }
}

#[test]
fn opens_any_transparency_reports_what_the_shell_needs_to_know() {
    assert!(opens_any_transparency(&alpha_plan(&regions())));
    let opaque_only = WindowRegions {
        client: client(),
        banner: Some(client()),
        opaque_islands: vec![],
    };
    assert!(!opens_any_transparency(&alpha_plan(&opaque_only)));
}
