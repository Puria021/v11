/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      fontFamily: { sans: ["Vazirmatn", "Tahoma", "sans-serif"] },
      colors: {
        brand: { 50: "#eef5ff", 100: "#d9e8ff", 500: "#2f6fed", 600: "#2259d1", 700: "#1d48a8" },
      },
    },
  },
  plugins: [],
};
