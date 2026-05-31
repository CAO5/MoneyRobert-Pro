/** @type {import('tailwindcss').Config} */
export default {
  darkMode: "class",
  content: ["./index.html", "./src/**/*.{js,ts,vue}"],
  theme: {
    container: { center: true },
    extend: {
      colors: {
        primary: { DEFAULT: '#0B0F19', secondary: '#111622', tertiary: '#1A1F2E', elevated: '#232A3B' },
        border: { subtle: 'rgba(255,255,255,0.06)', DEFAULT: 'rgba(255,255,255,0.1)', hover: 'rgba(255,255,255,0.15)', accent: 'rgba(212,175,55,0.3)' },
        gold: { DEFAULT: '#D4AF37', dim: '#8B7355', glow: 'rgba(212,175,55,0.15)', hover: '#E5C04A' },
        profit: '#00C853',
        loss: '#FF1744',
      },
      fontFamily: {
        sans: ['Inter', '-apple-system', 'BlinkMacSystemFont', '"Segoe UI"', 'sans-serif'],
        mono: ['"JetBrains Mono"', '"SF Mono"', 'Monaco', 'monospace'],
        display: ['"Cormorant Garamond"', '"Times New Roman"', 'serif'],
        chinese: ['"PingFang SC"', '"Microsoft YaHei"', 'sans-serif'],
      },
    },
  },
  plugins: [],
}
