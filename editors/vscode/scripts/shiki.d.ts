// The part of Shiki's `LanguageRegistration` that website/lib/sakura/grammar.ts uses.
export type LanguageRegistration = {
  name: string;
  displayName?: string;
  aliases?: string[];
  scopeName: string;
  [key: string]: unknown;
};
