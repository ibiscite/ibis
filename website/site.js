// Apply the local preference before paint; the complete page works without it.
try {
  const choice = localStorage.getItem("ibis-theme");
  if (choice === "light" || choice === "dark") document.documentElement.dataset.theme = choice;
} catch { /* Storage is optional. */ }
const platform = navigator.userAgentData?.platform ?? navigator.platform;
if (/Mac/i.test(platform)) document.documentElement.dataset.platform = "macos";
else if (/Win/i.test(platform)) document.documentElement.dataset.platform = "windows";
else if (/Linux/i.test(platform)) document.documentElement.dataset.platform = "linux";
