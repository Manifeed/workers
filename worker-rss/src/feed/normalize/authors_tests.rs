use super::{normalize_author_identity, split_author_value};

#[test]
fn split_author_value_expands_obvious_composite_names() {
    assert_eq!(
        split_author_value("Alice Martin, Bob Stone and Chloé Durand"),
        vec![
            "Alice Martin".to_string(),
            "Bob Stone".to_string(),
            "Chloé Durand".to_string(),
        ]
    );
}

#[test]
fn split_author_value_discards_locations_and_credit_suffixes() {
    assert_eq!(
        split_author_value("Adélie Aubaret avec AFP, à Bastia"),
        vec!["Adélie Aubaret".to_string()]
    );
    assert_eq!(
        split_author_value("Anne Le Nir, au Liban"),
        vec!["Anne Le Nir".to_string()]
    );
    assert_eq!(
        split_author_value("Anne Le Nir au Liban"),
        vec!["Anne Le Nir".to_string()]
    );
    assert_eq!(
        split_author_value("avec Agnès Rotivel"),
        vec!["Agnès Rotivel".to_string()]
    );
    assert_eq!(
        split_author_value("Axel Chouvel avec Orthodoxie.com"),
        vec!["Axel Chouvel".to_string()]
    );
    assert_eq!(
        split_author_value("Jean Dupont, correspondant à Kiev"),
        vec!["Jean Dupont".to_string()]
    );
    assert_eq!(
        split_author_value("Jean Dupont envoyée spéciale à Gaza"),
        vec!["Jean Dupont".to_string()]
    );
    assert_eq!(
        split_author_value("Delphine Nerbollier à Berlin"),
        vec!["Delphine Nerbollier".to_string()]
    );
    assert_eq!(
        split_author_value("Delphine Nerbollier à Stuttgart"),
        vec!["Delphine Nerbollier".to_string()]
    );
    assert_eq!(
        split_author_value("Delphine Nerbollier correspondante"),
        vec!["Delphine Nerbollier".to_string()]
    );
    assert_eq!(
        split_author_value("Delphine Nerbollier (à Berlin)"),
        vec!["Delphine Nerbollier".to_string()]
    );
}

#[test]
fn split_author_value_strips_editorial_prefixes() {
    assert_eq!(
        split_author_value("de notre correspondant au Liban"),
        vec!["correspondant au Liban".to_string()]
    );
    assert_eq!(
        split_author_value("propos recueillis par Jean Dupont"),
        vec!["Jean Dupont".to_string()]
    );
    assert_eq!(
        split_author_value("recueilli par J. D."),
        vec!["J. D.".to_string()]
    );
    assert_eq!(
        split_author_value("reported by AFP"),
        vec!["AFP".to_string()]
    );
    assert_eq!(
        split_author_value("Text John Doe"),
        vec!["John Doe".to_string()]
    );
    assert_eq!(
        split_author_value("edited by sarah smaje executive producers are molly glassey"),
        vec!["sarah smaje".to_string(), "molly glassey".to_string()]
    );
    assert_eq!(
        split_author_value("Olivier-Marie JOSEPH aumônier d’hôpital (diocèse de Créteil)"),
        vec!["Olivier-Marie JOSEPH".to_string()]
    );
}

#[test]
fn split_author_value_keeps_non_author_conjunction_labels_whole() {
    assert_eq!(
        split_author_value("Research and Markets"),
        vec!["Research and Markets".to_string()]
    );
}

#[test]
fn split_author_value_discards_generic_editorial_labels() {
    assert_eq!(
        split_author_value("A Guardian reporter"),
        Vec::<String>::new()
    );
    assert_eq!(
        split_author_value("notre correspondant régional"),
        Vec::<String>::new()
    );
}

#[test]
fn normalize_author_identity_removes_accents_and_collapses_spacing() {
    assert_eq!(
        normalize_author_identity("  Chloé   Dùrand  "),
        Some("chloe durand".to_string())
    );
}
