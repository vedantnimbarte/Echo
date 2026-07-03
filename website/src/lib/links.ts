// Single source of truth for external project links used across the site.
// The canonical repository is derived once so release/doc URLs stay in sync.

export const REPO_URL = "https://github.com/vedantnimbarte/Echo";

export const LINKS = {
  github: REPO_URL,
  releases: `${REPO_URL}/releases/latest`,
  contributing: `${REPO_URL}/blob/main/CONTRIBUTING.md`,
  plugins: `${REPO_URL}/blob/main/PLUGINS.md`,
  license: `${REPO_URL}/blob/main/LICENSE`,
  issues: `${REPO_URL}/issues`,
} as const;
