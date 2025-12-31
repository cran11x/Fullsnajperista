# Fix GitHub Credentials Issue

Your source control is showing "bad credentials" because GitHub no longer accepts passwords for HTTPS authentication.

## Quick Fix Steps:

### Step 1: Remove Old Credentials
1. Press `Win + R`
2. Type: `control /name Microsoft.CredentialManager`
3. Press Enter
4. Click "Windows Credentials"
5. Find and **delete** any entries containing:
   - `github.com`
   - `git:https://github.com`
   - `GitHub for Visual Studio`

### Step 2: Create a Personal Access Token (PAT)
1. Go to: https://github.com/settings/tokens/new
2. Click "Generate new token" → "Generate new token (classic)"
3. Give it a name like "VS Code" or "Cursor IDE"
4. Select the **`repo`** scope (full control of private repositories)
5. Click "Generate token" at the bottom
6. **COPY THE TOKEN IMMEDIATELY** (you won't see it again!)

### Step 3: Use the Token
- When your editor/source control asks for credentials:
  - **Username:** Your GitHub username (cran11x)
  - **Password:** Paste your Personal Access Token (NOT your GitHub password)

The credentials will be saved automatically by Windows Credential Manager.

---

**Alternative:** If you prefer SSH (no password/token needed once set up):
1. Add your SSH public key to GitHub: https://github.com/settings/keys
2. Your SSH key is located at: `C:\Users\danis\.ssh\id_ed25519.pub`
3. View it with: `Get-Content C:\Users\danis\.ssh\id_ed25519.pub`

