/**
 * Props for a field that holds names and tags exactly as typed. macOS's text system otherwise
 * corrects spellings ("electronica" -> "Electronica"), capitalises the first letter and marks
 * artist names as misspelt, in the app's web view as in Safari.
 */
export const PLAIN_TEXT = { autoCorrect: "off", autoCapitalize: "off", spellCheck: false } as const;
