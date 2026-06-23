// tailwind.config.js
/** @type {import('tailwindcss').Config} */
module.exports = {
  content: [
    "./src/**/*.{rs,html}",  // Inclut tous les fichiers Rust/HTML dans src/
    "index.html",          // Si vous avez un fichier index.html
  ],
  theme: {
    extend: {
        keyframes: {
          blink: {
            '0%, 100%': { opacity: 1 },
            '50%': { opacity: 0 },
          },
        },
        animation: {
          blink: 'blink 1s infinite',
        },
        sans: ['Marianne', 'ui-sans-serif', 'system-ui', 'sans-serif']
    },
  },
  plugins: [],
}