// Shared type-icon drawings for native and browser surfaces.
export const typePaths = {
  article: ["M5 2.5h7l3 3v12H5z", "M8 8.5h4.5M8 11.5h4.5M8 14.5h3"],
  book: ["M4 4.5A2 2 0 0 1 6 2.5h9.5v12H6a2 2 0 0 0-2 2z", "M4 16.5a2 2 0 0 0 2 1h9.5"],
  chapter: ["M4 4.5A2 2 0 0 1 6 2.5h9.5v12H6a2 2 0 0 0-2 2z", "M4 16.5a2 2 0 0 0 2 1h9.5", "M9 2.5v6l2-1.5 2 1.5v-6"],
  thesis: ["m10 3.5 8 4-8 4-8-4z", "M5 9v4c0 1.4 2.2 2.5 5 2.5s5-1.1 5-2.5V9"],
  web: ["M2.5 5.5a2 2 0 0 1 2-2h11a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-11a2 2 0 0 1-2-2z", "M2.5 7.5h15"],
  report: ["M7.5 3.5H5v14h10v-14h-2.5", "M7.5 2.5h5V5h-5z", "M8 14.5v-2.5M10 14.5v-5M12 14.5V11"],
  other: ["M10 2.5 16.5 6v8L10 17.5 3.5 14V6z", "M3.5 6 10 9.5 16.5 6M10 9.5v8"],
} as const;
