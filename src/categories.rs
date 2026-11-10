//! Standard Newznab category IDs sent to the indexer, not client-side filters.
pub const ROOTS: &[(u32, &str)] = &[
    (0, "All"),
    (2000, "Movies"),
    (5000, "TV"),
    (3000, "Music"),
    (7000, "Books"),
    (4000, "Apps"),
    (1000, "Games"),
    (8000, "Other"),
    (6000, "Adult"),
];
pub fn children(root: u32) -> &'static [(u32, &'static str)] {
    match root {
        2000 => &[
            (2030, "SD"),
            (2040, "HD"),
            (2045, "UHD / 4K"),
            (2050, "Blu-ray"),
            (2060, "3D"),
            (2010, "Foreign"),
            (2020, "Other"),
        ],
        5000 => &[
            (5030, "SD"),
            (5040, "HD"),
            (5045, "UHD / 4K"),
            (5060, "Sport"),
            (5070, "Anime"),
            (5080, "Documentary"),
            (5020, "Foreign"),
            (5050, "Other"),
        ],
        3000 => &[
            (3010, "MP3"),
            (3020, "Video"),
            (3030, "Audiobooks"),
            (3040, "Lossless"),
            (3050, "Other"),
        ],
        7000 => &[
            (7010, "Magazines"),
            (7020, "Ebooks"),
            (7030, "Comics"),
            (7040, "Technical"),
            (7050, "Other"),
        ],
        4000 => &[
            (4010, "Windows"),
            (4020, "Mac"),
            (4030, "Mobile"),
            (4040, "Games"),
            (4050, "Other"),
            (4060, "iOS"),
            (4070, "Android"),
        ],
        1000 => &[
            (1010, "NDS"),
            (1020, "PSP"),
            (1030, "Wii"),
            (1040, "Xbox"),
            (1050, "Xbox 360"),
            (1080, "PS3"),
            (1090, "Other"),
        ],
        _ => &[],
    }
}
pub fn name(id: u32) -> String {
    for &(root, name) in ROOTS {
        if id == root {
            return if root == 0 {
                "All categories".into()
            } else {
                name.into()
            };
        }
        if let Some((_, child)) = children(root).iter().find(|(v, _)| *v == id) {
            return format!("{name} / {child}");
        }
    }
    if id == 0 {
        "Uncategorized".into()
    } else {
        format!("Category {id}")
    }
}
