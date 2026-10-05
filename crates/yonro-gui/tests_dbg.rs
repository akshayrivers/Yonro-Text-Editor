#[test]
fn dbg_lens() {
    let dir = std::env::temp_dir().join("yonro-gui-dbg");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut project = yonro_core::Project::load(&dir);
    project.manuscript = yonro_core::Manuscript::new("Probe");
    let act = project.manuscript.add_act("Act I").unwrap();
    let ch = project.manuscript.add_chapter(act, "C").unwrap();
    let sc = project.manuscript.add_scene(ch, "Gate").unwrap();
    project.manuscript.set_meta(sc, yonro_core::SceneMeta { pov: "Mara".to_string(), ..Default::default() }).unwrap();
    project.save().unwrap();
    project.add_entity("character", "Mara").unwrap();
    eprintln!("lore count: {}", project.lore.live_count());
    eprintln!("resolve: {:?}", project.lore.resolve("Mara").map(|e| e.id));
    let texts = std::collections::BTreeMap::new();
    let g = yonro_core::graph::Graph::build(&project.manuscript, &project.lore, &texts);
    eprintln!("nodes: {} edges: {}", g.nodes.len(), g.edges.len());
}
