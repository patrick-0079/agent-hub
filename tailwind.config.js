/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        // 中性灰阶（扁平、无色偏）
        ink: {
          950: "#0a0b0d",
          900: "#101214",
          850: "#15171a",
          800: "#1c1f23",
          700: "#262a2f",
          600: "#343941",
          500: "#454c55",
        },
        // 单一强调色（低饱和青）
        brand: {
          900: "#0d2723",
          800: "#14413a",
          500: "#2f9d90",
          400: "#4db3a6",
        },
        // 次强调（低饱和紫，仅用于“规划/待实现”语义）
        accent: {
          900: "#1d1a30",
          800: "#2b2745",
          500: "#8a7fbe",
          400: "#a89dd0",
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
        "fade-in": "fade-in 0.18s ease-out",
        "pulse-soft": "pulse-soft 1.6s ease-in-out infinite",
        sweep: "sweep 1.4s linear infinite",
      },
    },
  },
  plugins: [],
};
