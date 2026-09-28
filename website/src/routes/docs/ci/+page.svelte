<script lang="ts">
	import Callout from '$lib/components/Callout.svelte';
	import CodeBlock from '$lib/components/CodeBlock.svelte';
	import Seo from '$lib/components/Seo.svelte';
	import { ciBuild, ciInstall } from '$lib/snippets';

	const missingPermission = `permissions:
  id-token: write`;

	const dockerJob = `      - uses: actions/checkout@v5
${ciInstall('cargo-credential-privatecrates')}      # Fetch private and public dependencies here, where the OIDC token is available.
      - run: mkdir -p .cargo && cargo vendor --locked vendor >> .cargo/config.toml
      # The build context now holds every dependency; the image build needs no registry access.
      - run: docker build -t ghcr.io/acme/story-app:\${{ github.sha }} .`;

	const dockerfile = `FROM rust:1 AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked --offline`;
</script>

<Seo
	title="CI without secrets"
	description="Read private crates from GitHub Actions with OIDC: no secrets to store or rotate. Plus Docker builds and CI outside GitHub."
	path="/docs/ci"
/>

<h1>CI without secrets</h1>
<p class="lede">
	GitHub Actions gives each job a short-lived OIDC token. The credential provider uses it, so there is nothing
	to create, store or rotate: no App, no secret, no action of ours.
</p>

<h2 id="workflow">The workflow</h2>
<p>
	Add <code>id-token: write</code> to the job’s permissions and install the credential provider. The step
	below downloads the prebuilt binary from our GitHub release, pinned to a version, and checks it against the
	release’s checksums and GitHub’s build attestation before it runs: a few seconds, and no third-party action.
	On a runner other than Linux, <code>cargo install cargo-credential-privatecrates --locked</code> builds it instead.
</p>
<CodeBlock caption=".github/workflows/build.yml" code={ciBuild} />
<p>
	The same <code>.cargo/config.toml</code> your developers use is all Cargo needs. The workflow uses no
	third-party actions beyond <code>actions/checkout</code>: the job that holds your publish token runs only
	code from crates.io and GitHub.
</p>

<h2 id="how">What happens</h2>
<ol>
	<li>
		The provider notices it is in Actions (from <code>ACTIONS_ID_TOKEN_REQUEST_URL</code>) and asks GitHub for
		an OIDC token whose audience is your registry, <code>https://acme.privatecrates.dev</code>.
	</li>
	<li>
		It exchanges that token at the registry for a <strong>read-only registry token</strong>, valid for one
		hour and cached until it expires.
	</li>
	<li>
		Cargo uses it for index reads and downloads. The registry checks the job’s repository belongs to your
		organisation and applies your <code>ci_read</code> setting.
	</li>
</ol>
<p>
	Permission checks for CI use our App’s installation tokens, so CI never consumes a developer’s GitHub rate
	limit.
</p>

<h3 id="missing-permission">If <code>id-token: write</code> is missing</h3>
<p>
	The provider fails straight away with a message naming the line to add, rather than falling back to a
	browser sign-in that would hang the job:
</p>
<CodeBlock caption="workflow or job" code={missingPermission} />

<Callout title="Pull requests from forks">
	<p>
		GitHub does not give workflows triggered from forks an OIDC token, so they cannot read private crates.
		That is intended: a stranger’s pull request should not be able to download your code.
	</p>
</Callout>

<h2 id="elsewhere">Docker builds and CI outside GitHub Actions</h2>
<p>
	GitHub Actions reads the registry with no secret at all, so the safest way to build elsewhere is to let an
	Actions job fetch the dependencies, in one of two ways. For a platform that builds your image itself from
	Git, such as Railway or Render, see <a href="/docs/deploy">Deploying apps</a>: it needs a narrow, read-only
	GitHub token.
</p>

<h3>Build the image in GitHub Actions</h3>
<p>
	Let the Actions job fetch the dependencies with <code>cargo vendor</code>, which also writes the Cargo
	configuration that points at the vendored copies. The image then builds offline, and no registry token ever
	reaches Docker. Push the result to the container registry your platform deploys from.
</p>
<CodeBlock caption="workflow steps" code={dockerJob} />
<CodeBlock caption="Dockerfile" code={dockerfile} />

<h3>Or hand the vendored tree to another system</h3>
<p>
	The same <code>vendor</code> directory and configuration can be uploaded as an artifact for an external build,
	which then needs no registry access at all.
</p>
<p>
	If a build outside Actions must ever receive a registry token, pass it through a BuildKit secret mount (<code
		>RUN --mount=type=secret</code
	>), never a build argument: build arguments can be recorded in the image history.
</p>

<Callout tone="warn" title="Platforms that only offer build arguments">
	<p>
		Some hosting platforms expose build-time variables only as Docker build arguments. Never give such a build
		a registry token; use one of the two routes above instead.
	</p>
</Callout>
