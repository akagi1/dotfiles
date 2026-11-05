---
name: translate
description: Translate any given text into Polish — first the full translation, then a word-by-word breakdown. Use when the user invokes /translate or asks to translate something into Polish with a per-word breakdown.
---

# Translate to Polish

Translate the text the user provides (the arguments after `/translate`, or the text in their message) into Polish.

## Output format

### 1. Full translation

Give a complete, natural, fluent Polish translation of the whole text. Keep the meaning, tone and register of the original (formal/informal). Don't translate it word by word here, translate it the way a native speaker would say it.

### 2. Word by word

Then go through every word of the original text, in order, as a table:

| Originał | Angielski | Tłumaczenia (po polsku) |
|----------|-----------|-------------------------|

Column headers are always in Polish, exactly as above.

- **Originał**: the word as written in the source text.
- **Angielski**: the English equivalent of the word. If the source text is English, this column holds the original English word's meaning in context (a plain English gloss).
- **Tłumaczenia (po polsku)**: the Polish equivalent **in the form used in the full translation** (the right case, gender, tense, etc.), with the base/dictionary form in parentheses if it differs, e.g. `kota (kot)`, followed by a short explanation of its meaning and use written in Polish, as a Polish dictionary or teacher would put it (e.g. for *męka*: "silne cierpienie fizyczne lub psychiczne"). If a word has no direct equivalent or gets dropped in Polish (e.g. articles "a", "the"), write `—` and say so briefly.

Prepositions (e.g. "in", "on", "na", "do", "w") don't get their own row. Skip them. Explain them only as part of an idiom or phrase row when they change its meaning.

Treat idioms and fixed phrases (e.g. "give up", "by the way") as one row, translate them as a unit, then explain what the individual words mean literally.

## Rules

- If the input is already Polish, translate it into English instead and do the word-by-word breakdown the same way (Polish → English). Keep the **Tłumaczenia (po polsku)** column in Polish.
- For long inputs (more than a few sentences), do the word-by-word part one sentence at a time, with each sentence as a small heading above its table.
- Don't add anything else: no intro, no closing remarks.
