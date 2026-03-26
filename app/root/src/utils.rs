use rand::Rng;

pub fn generate_complex_filename() -> String {
    let mut rng = rand::thread_rng();
    
    const CHARSET_CYRILLIC_LOWER: &str = "абвгдеёжзийклмнопрстуфхцчшщъыьэюя";
    const CHARSET_CYRILLIC_UPPER: &str = "АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯ";
    const CHARSET_LATIN_LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
    const CHARSET_LATIN_UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    const CHARSET_NUMBERS: &str = "0123456789";

    let mut chars: Vec<char> = Vec::new();
    chars.extend(CHARSET_CYRILLIC_LOWER.chars());
    chars.extend(CHARSET_CYRILLIC_UPPER.chars());
    chars.extend(CHARSET_LATIN_LOWER.chars());
    chars.extend(CHARSET_LATIN_UPPER.chars());
    chars.extend(CHARSET_NUMBERS.chars());

    let random_string: String = (0..10)
        .map(|_| {
            let idx = rng.gen_range(0..chars.len());
            chars[idx]
        })
        .collect();

    format!("{}.webp", random_string)
}
