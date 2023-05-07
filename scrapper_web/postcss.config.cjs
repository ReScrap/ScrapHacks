let cssnano_plugin = {};
if (process.env.NODE_ENV === "production") {
  cssnano_plugin = { cssnano: { preset: "advanced" } };
}
module.exports = {
  plugins: {
    tailwindcss: {},
    autoprefixer: {},
    ...cssnano_plugin,
  },
};
