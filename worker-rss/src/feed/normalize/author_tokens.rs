pub(super) fn is_location_preposition(token: &str) -> bool {
    matches!(
        token,
        "in" | "at" | "a" | "au" | "aux" | "en" | "dans" | "depuis" | "sur"
    )
}

pub(super) fn is_descriptor_cutoff_token(token: &str) -> bool {
    is_location_preposition(token)
        || matches!(
            token,
            "aumonier"
                | "aumoniere"
                | "diocese"
                | "editor"
                | "editors"
                | "edited"
                | "executive"
                | "producer"
                | "producers"
                | "reporter"
                | "regional"
                | "regionale"
                | "special"
                | "speciale"
                | "speciales"
                | "specialiste"
                | "correspondance"
                | "correspondant"
                | "correspondante"
                | "hospital"
                | "hopital"
                | "bureau"
        )
}

pub(super) fn is_name_particle(token: &str) -> bool {
    matches!(
        token,
        "de" | "du"
            | "des"
            | "del"
            | "della"
            | "di"
            | "da"
            | "van"
            | "von"
            | "bin"
            | "ibn"
            | "al"
            | "la"
            | "le"
    )
}
