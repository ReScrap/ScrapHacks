module.exports = {
  content: ["./src/**/*.{svelte,js,ts}"],
  plugins: [require("@tailwindcss/forms"),require("daisyui")],
  theme: {
    container: {
      center: true,
    },
  },
  daisyui: {
    styled: true,
    themes: true,
    base: true,
    utils: true,
    logs: true,
    rtl: false,
    prefix: "",
    darkTheme: "scraptool",
    themes: [
      {
        scraptool: {
          primary: "#F28C18",
          secondary: "#b45309",
          accent: "#22d3ee",
          neutral: "#1B1D1D",
          "base-100": "#212121",
          info: "#2463EB",
          success: "#16A249",
          warning: "#DB7706",
          error: "#DC2828",
          // "--rounded-box": "0.4rem",
          // "--rounded-btn": "0.2rem"
        },
      },
    ],
  },
};
