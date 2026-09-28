/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        ink: {
          950: "#070a10",
          900: "#0b0f17",
          850: "#0f1420",
          800: "#141a28",
          700: "#1c2434",
          600: "#2a3446",
          500: "#3b4759",
        },
        brand: {
          400: "#5eead4",
          500: "#2dd4bf",
          600: "#14b8a6",
        },
        accent: {
          400: "#a78bfa",
          500: "#8b5cf6",
        },
      },
      fontFamily: {
        sans: ["Inter", "Segoe UI", "system-ui", "sans-serif"],
        mono: ["Cascadia Code", "Consolas", "ui-monospace", "monospace"],
      },
      keyframes: {
        "fade-in": {
          "0%": { opacity: "0", transform: "translateY(4px)" },
          "100%": { opacity: "1", transform: "translateY(0)" },
        },
        "pulse-soft": {
          "0%, 100%": { opacity: "1" },
          "50%": { opacity: "0.45" },
        },
        sweep: {
          "0%": { transform: "translateX(-100%)" },
          "100%": { transform: "translateX(300%)" },
        },
      },
      animation: {
        "fade-in": "fade-in 0.22s ease-out",
        "pulse-soft": "pulse-soft 1.6s ease-in-out infinite",
        sweep: "sweep 1.4s linear infinite",
      },
    },
  },
  plugins: [],
};