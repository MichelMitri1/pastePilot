//! Removes generic AI phrasing before a reply is pasted
//! ("Certainly!", "It appears that", "I hope this helps."). Conservative on
//! purpose: it only touches stock phrases, never the substance of a reply.

/// Openers removed at the very start (or right after a short greeting like "Hey Sam!").
const OPENERS: &[&str] = &[
    "certainly!", "certainly,", "certainly.", "absolutely!", "of course!", "of course,", "great question!",
    "good question!", "thanks for reaching out!", "thank you for reaching out!", "thank you for reaching out.",
    "thanks for reaching out.", "i hope this message finds you well.", "i hope this message finds you well!",
    "i hope you're doing well.", "i hope you are doing well.", "i understand your concern.",
    "i understand your frustration.", "i completely understand.", "thank you for your patience.",
    "i'd be happy to help!", "i'd be happy to help.", "i would be happy to help!", "i would be happy to help.",
    "happy to help!",
];

/// Whole sentences dropped wherever they appear.
const DROP_SENTENCES: &[&str] = &[
    "i hope this helps!", "i hope this helps.", "i hope this information helps.", "i hope that helps!",
    "i hope that helps.", "please don't hesitate to reach out if you have any other questions.",
    "please don't hesitate to reach out if you have any further questions.",
    "please do not hesitate to reach out if you have any other questions.",
    "don't hesitate to reach out if you need further assistance.",
    "feel free to reach out if you have any other questions.",
    "feel free to reach out if you have any further questions.",
    "feel free to reach out if you have any more questions.",
    "as an ai language model,", "as an ai,",
];

/// Robotic wording → plain wording.
const REPLACEMENTS: &[(&str, &str)] = &[
    ("It appears that ", "It looks like "),
    ("it appears that ", "it looks like "),
    ("It seems that ", "It looks like "),
    ("it seems that ", "it looks like "),
    ("Please note that ", ""),
    ("please note that ", ""),
    ("Please be advised that ", ""),
    ("It is important to note that ", ""),
    ("It's important to note that ", ""),
    ("In order to ", "To "),
    (" in order to ", " to "),
    ("Additionally, ", "Also, "),
    ("Furthermore, ", "Also, "),
    ("Moreover, ", "Also, "),
    ("Kindly ", ""),
    (" kindly ", " "),
    ("utilize", "use"),
    ("utilizing", "using"),
    ("utilized", "used"),
];

pub fn remove(text: &str) -> String {
    let mut out = text.trim().to_string();

    // Openers, possibly after a short greeting ("Hey!", "Hi Sam,").
    loop {
        let (greeting, rest) = split_greeting(&out);
        let lower = rest.to_lowercase();
        let Some(op) = OPENERS.iter().find(|o| lower.starts_with(*o)) else { break };
        let remainder = rest[op.len()..].trim_start();
        out = format!("{greeting}{}", capitalize(remainder));
    }

    for sentence in DROP_SENTENCES {
        out = remove_ci(&out, sentence);
    }
    for (from, to) in REPLACEMENTS {
        out = out.replace(from, to);
    }
    tidy(&out)
}

/// "Hey Sam! rest" → ("Hey Sam! ", "rest"). Only short greetings count.
fn split_greeting(s: &str) -> (String, &str) {
    let lower = s.to_lowercase();
    if ["hey", "hi", "hello"].iter().any(|g| lower.starts_with(g)) {
        if let Some(end) = s.find(['!', ',', '.', '\n']) {
            if end <= 30 {
                let mut cut = end + 1;
                while s[cut..].starts_with([' ', '\n']) {
                    cut += 1;
                }
                return (s[..cut].to_string(), &s[cut..]);
            }
        }
    }
    (String::new(), s)
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Case-insensitive removal of a phrase (ASCII phrases, so byte offsets line up).
fn remove_ci(text: &str, phrase: &str) -> String {
    let lower = text.to_lowercase();
    if lower.len() != text.len() {
        return text.to_string(); // non-ASCII case changes shift offsets; skip safely
    }
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while let Some(pos) = lower[i..].find(phrase) {
        out.push_str(&text[i..i + pos]);
        i += pos + phrase.len();
    }
    out.push_str(&text[i..]);
    out
}

fn tidy(s: &str) -> String {
    let mut lines: Vec<String> = s
        .lines()
        .map(|l| {
            let mut l = l.to_string();
            while l.contains("  ") {
                l = l.replace("  ", " ");
            }
            let t = l.trim_end().to_string();
            // Capitalize a line that now starts lowercase after a removal.
            if t.trim_start() != t {
                t
            } else {
                capitalize(&t)
            }
        })
        .collect();
    // Drop blank lines left at the ends and collapse runs of blank lines.
    lines.dedup_by(|a, b| a.trim().is_empty() && b.trim().is_empty());
    while lines.first().is_some_and(|l| l.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    lines.join("\n").replace(" .", ".").replace(" ,", ",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_openers_and_closers() {
        assert_eq!(remove("Certainly! It appears that your import is wrong. I hope this helps!"), "It looks like your import is wrong.");
        assert_eq!(remove("Hey Sam! Great question! Try deleting node_modules :)"), "Hey Sam! Try deleting node_modules :)");
    }

    #[test]
    fn replaces_robotic_wording() {
        assert_eq!(remove("In order to fix it, utilize the --force flag."), "To fix it, use the --force flag.");
        assert_eq!(remove("Please note that the course is 12 weeks."), "The course is 12 weeks.");
    }

    #[test]
    fn leaves_normal_replies_alone() {
        let r = "Hey! Try running npm install again :)\n\n1. Delete node_modules\n2. Run npm install";
        assert_eq!(remove(r), r);
        assert_eq!(remove("Sure thing! Send me the error and I'll take a look."), "Sure thing! Send me the error and I'll take a look.");
    }
}
