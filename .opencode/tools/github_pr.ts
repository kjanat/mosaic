import { graphql } from '@octokit/graphql';
import { tool } from '@opencode-ai/plugin';

const DEFAULT_OWNER = 'kjanat';
const DEFAULT_REPOSITORY = 'mosaic';
const GRAPHQL_TIMEOUT_MS = 30_000;
const DEFAULT_PR_LIMIT = 20;
const PHASE_OPTIONS = 'MVP 0, MVP 1, MVP 2, MVP 3, MVP 4, MVP 5, MVP 6, Later';
const PHASE_GUIDE = [
	'MVP 0: core compiler skeleton — .mos -> document graph -> simple PDF; parser, sections, paragraphs, basic layout/PDF, diagnostics.',
	'MVP 1: references and counters — labels, references, section/figure/equation numbering, internal fixpoint loop.',
	'MVP 2: real text layout — font loading, shaping, line breaking, hyphenation, paragraph layout cache.',
	'MVP 3: figures and floats — images, captions, float placement constraints, list of figures, debug float diagnostics.',
	'MVP 4: bibliography — BibTeX import, CSL styles, citation clusters, bibliography rendering.',
	'MVP 5: incremental builds — dependency graph, stable node IDs, dirty-node invalidation, persistent cache, watch mode.',
	'MVP 6: editor integration — LSP, live preview, source/PDF sync.',
	'Later: parked manifest ideas and non-sequenced work; package systems/plugins stay here unless explicitly pulled forward.',
];

type JsonRecord = Record<string, unknown>;

type RepositoryDefaults = {
	owner: string;
	repository: string;
};

type PullRequestSummary = {
	number: number;
	title: string;
	url: string;
	author: string;
	state: string;
	draft: boolean;
	reviewDecision: string;
	mergeState: string;
	checks: string;
	changedFiles: number;
	additions: number;
	deletions: number;
};

type IssueSummary = {
	number: number;
	title: string;
	state: string;
	url: string;
	source: string;
};

type Classification = {
	area: string;
	type: string;
	size: string;
	priority: string;
	reason: string;
};

function defaults(owner: string | undefined, repository: string | undefined): RepositoryDefaults {
	return {
		owner: owner ?? DEFAULT_OWNER,
		repository: repository ?? DEFAULT_REPOSITORY,
	};
}

function isRecord(value: unknown): value is JsonRecord {
	return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function recordValue(record: JsonRecord, key: string): JsonRecord | null {
	const value = record[key];
	return isRecord(value) ? value : null;
}

function arrayField(record: JsonRecord, key: string): ReadonlyArray<unknown> {
	const value = record[key];
	return Array.isArray(value) ? value : [];
}

function stringField(record: JsonRecord, key: string): string | null {
	const value = record[key];
	return typeof value === 'string' ? value : null;
}

function numberField(record: JsonRecord, key: string): number | null {
	const value = record[key];
	return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function booleanField(record: JsonRecord, key: string): boolean | null {
	const value = record[key];
	return typeof value === 'boolean' ? value : null;
}

function errorMessage(error: unknown): string {
	if (error instanceof Error) {
		return error.message;
	}
	return String(error);
}

let cachedToken: string | null = null;

async function githubToken(): Promise<string> {
	const envToken = process.env.GITHUB_TOKEN ?? process.env.GH_TOKEN;
	if (envToken !== undefined && envToken.trim() !== '') {
		return envToken.trim();
	}
	if (cachedToken !== null) {
		return cachedToken;
	}
	const token = (await Bun.$`gh auth token`.text()).trim();
	if (token === '') {
		throw new Error('gh auth token returned an empty token');
	}
	cachedToken = token;
	return token;
}

async function githubGraphql(query: string, variables: JsonRecord): Promise<JsonRecord> {
	const token = await githubToken();
	const graphqlWithAuth = graphql.defaults({
		headers: {
			authorization: `bearer ${token}`,
			'user-agent': 'mosaic-opencode-tools',
		},
		request: {
			timeout: GRAPHQL_TIMEOUT_MS,
		},
	});
	const data: unknown = await graphqlWithAuth(query, variables);
	if (!isRecord(data)) {
		throw new Error('GitHub GraphQL returned non-object data');
	}
	return data;
}

function repositoryNode(data: JsonRecord): JsonRecord {
	const repository = recordValue(data, 'repository');
	if (repository === null) {
		throw new Error('GitHub GraphQL response did not include repository');
	}
	return repository;
}

function pullRequestNode(data: JsonRecord, number: number): JsonRecord {
	const repository = repositoryNode(data);
	const pr = recordValue(repository, 'pullRequest');
	if (pr === null) {
		throw new Error(`Pull request #${number} not found`);
	}
	return pr;
}

function issueNode(data: JsonRecord, number: number): JsonRecord | null {
	const repository = repositoryNode(data);
	return recordValue(repository, 'issue');
}

function nodeList(connection: JsonRecord | null): ReadonlyArray<JsonRecord> {
	if (connection === null) {
		return [];
	}
	return arrayField(connection, 'nodes').filter(isRecord);
}

function labelNames(record: JsonRecord): ReadonlyArray<string> {
	return nodeList(recordValue(record, 'labels')).flatMap((label) => {
		const name = stringField(label, 'name');
		return name === null ? [] : [name];
	});
}

function filePaths(pr: JsonRecord): ReadonlyArray<string> {
	return nodeList(recordValue(pr, 'files')).flatMap((file) => {
		const path = stringField(file, 'path');
		return path === null ? [] : [path];
	});
}

function statusState(pr: JsonRecord): string {
	const rollup = recordValue(pr, 'statusCheckRollup');
	return rollup === null ? '' : stringField(rollup, 'state') ?? '';
}

function authorLogin(record: JsonRecord): string {
	const author = recordValue(record, 'author');
	return author === null ? '' : stringField(author, 'login') ?? '';
}

function summarizePullRequest(pr: JsonRecord): PullRequestSummary {
	return {
		number: numberField(pr, 'number') ?? 0,
		title: stringField(pr, 'title') ?? '',
		url: stringField(pr, 'url') ?? '',
		author: authorLogin(pr),
		state: stringField(pr, 'state') ?? '',
		draft: booleanField(pr, 'isDraft') ?? false,
		reviewDecision: stringField(pr, 'reviewDecision') ?? '',
		mergeState: stringField(pr, 'mergeStateStatus') ?? '',
		checks: statusState(pr),
		changedFiles: numberField(pr, 'changedFiles') ?? 0,
		additions: numberField(pr, 'additions') ?? 0,
		deletions: numberField(pr, 'deletions') ?? 0,
	};
}

function summarizeIssue(issue: JsonRecord, source: string): IssueSummary {
	return {
		number: numberField(issue, 'number') ?? 0,
		title: stringField(issue, 'title') ?? '',
		state: stringField(issue, 'state') ?? '',
		url: stringField(issue, 'url') ?? '',
		source,
	};
}

function markdownCell(value: string): string {
	return value.replace(/\|/g, '\\|');
}

function formatPrTable(items: ReadonlyArray<PullRequestSummary>): string {
	if (items.length === 0) {
		return 'No matching pull requests.';
	}
	const rows = items.map((item) => {
		const flags = [item.draft ? 'draft' : '', item.reviewDecision, item.mergeState, item.checks].filter(Boolean).join(
			', ',
		);
		return [`#${item.number}`, item.title, item.author, item.state, flags]
			.map(markdownCell)
			.join(' | ');
	});
	return [
		'| PR | Title | Author | State | Signals |',
		'| --- | --- | --- | --- | --- |',
		...rows.map((row) => `| ${row} |`),
	].join('\n');
}

function formatIssueTable(items: ReadonlyArray<IssueSummary>): string {
	if (items.length === 0) {
		return 'No related issues found.';
	}
	const rows = items.map((item) =>
		[`#${item.number}`, item.title, item.state, item.source, item.url].map(markdownCell).join(' | ')
	);
	return [
		'| Issue | Title | State | Source | URL |',
		'| --- | --- | --- | --- | --- |',
		...rows.map((row) => `| ${row} |`),
	].join('\n');
}

async function openPullRequests(repo: RepositoryDefaults, limit: number): Promise<ReadonlyArray<JsonRecord>> {
	const query = `query($owner: String!, $repo: String!, $first: Int!) {
    repository(owner: $owner, name: $repo) {
      pullRequests(first: $first, states: OPEN, orderBy: { field: UPDATED_AT, direction: DESC }) {
        nodes {
          number title url state isDraft changedFiles additions deletions
          reviewDecision mergeStateStatus
          author { login }
          statusCheckRollup { state }
        }
      }
    }
  }`;
	const data = await githubGraphql(query, { owner: repo.owner, repo: repo.repository, first: limit });
	return nodeList(recordValue(repositoryNode(data), 'pullRequests'));
}

async function pullRequest(repo: RepositoryDefaults, number: number): Promise<JsonRecord> {
	const query = `query($owner: String!, $repo: String!, $number: Int!) {
    repository(owner: $owner, name: $repo) {
      pullRequest(number: $number) {
        number title body url state isDraft baseRefName headRefName changedFiles additions deletions
        reviewDecision mergeStateStatus
        author { login }
        labels(first: 20) { nodes { name } }
        statusCheckRollup { state }
        closingIssuesReferences(first: 20) { nodes { number title state url } }
        files(first: 80) { nodes { path additions deletions changeType } }
      }
    }
  }`;
	return pullRequestNode(await githubGraphql(query, { owner: repo.owner, repo: repo.repository, number }), number);
}

async function issueByNumber(repo: RepositoryDefaults, number: number): Promise<JsonRecord | null> {
	const query = `query($owner: String!, $repo: String!, $number: Int!) {
    repository(owner: $owner, name: $repo) {
      issue(number: $number) { number title state url }
    }
  }`;
	return issueNode(await githubGraphql(query, { owner: repo.owner, repo: repo.repository, number }), number);
}

function mentionedIssueNumbers(pr: JsonRecord): ReadonlyArray<number> {
	const text = `${stringField(pr, 'title') ?? ''}\n${stringField(pr, 'body') ?? ''}`;
	const seen = new Set<number>();
	for (const match of text.matchAll(/(?:^|[^A-Za-z0-9_/-])#(\d+)\b/g)) {
		const raw = match[1];
		if (raw === undefined) {
			continue;
		}
		const number = Number.parseInt(raw, 10);
		if (Number.isFinite(number)) {
			seen.add(number);
		}
	}
	return [...seen].sort((left, right) => left - right);
}

async function relatedIssues(repo: RepositoryDefaults, pr: JsonRecord): Promise<ReadonlyArray<IssueSummary>> {
	const byNumber = new Map<number, IssueSummary>();
	for (const issue of nodeList(recordValue(pr, 'closingIssuesReferences'))) {
		const summary = summarizeIssue(issue, 'closing reference');
		if (summary.number !== 0) {
			byNumber.set(summary.number, summary);
		}
	}
	for (const number of mentionedIssueNumbers(pr)) {
		if (byNumber.has(number)) {
			continue;
		}
		const issue = await issueByNumber(repo, number);
		if (issue !== null) {
			byNumber.set(number, summarizeIssue(issue, 'body mention'));
		}
	}
	return [...byNumber.values()].sort((left, right) => left.number - right.number);
}

function pathScore(paths: ReadonlyArray<string>, matcher: (path: string) => boolean): number {
	return paths.filter(matcher).length;
}

function chooseArea(paths: ReadonlyArray<string>, title: string): string {
	const titleKey = title.toLowerCase();
	const scores: ReadonlyArray<[string, number]> = [
		['CLI', pathScore(paths, (path) => path.includes('crates/mos/src') || path.includes('crates/mos/'))],
		['Core', pathScore(paths, (path) => path.includes('mos-core'))],
		['Diagnostics', pathScore(paths, (path) => path.includes('diagnostic') || path.includes('codes.rs'))],
		['Parser/Syntax', pathScore(paths, (path) => path.includes('mos-parse') || path.endsWith('mosaic.ebnf'))],
		['Semantic/Resolver', pathScore(paths, (path) => path.includes('mos-eval'))],
		['Layout', pathScore(paths, (path) => path.includes('mos-layout'))],
		[
			'Fonts',
			pathScore(
				paths,
				(path) =>
					path.includes('mos-fonts') || path.includes('pdf-base14-metrics') || path.includes('adobe-font-metrics'),
			),
		],
		['PDF', pathScore(paths, (path) => path.includes('mos-pdf'))],
		['Figures/Floats', pathScore(paths, (path) => path.includes('figure') || path.includes('image_lower'))],
		['Bibliography', pathScore(paths, (path) => path.includes('mos-bib') || path.includes('bib'))],
		['Incremental Cache', pathScore(paths, (path) => path.includes('mos-cache') || path.includes('cache'))],
		[
			'Page Reflow/Fixpoints',
			pathScore(
				paths,
				(path) => path.includes('fixpoint') || path.includes('reflow') || path.includes('page-reference'),
			),
		],
		[
			'Other Backends',
			pathScore(paths, (path) => path.includes('mos-html') || path.includes('epub') || path.includes('svg')),
		],
		[
			'Determinism',
			pathScore(
				paths,
				(path) => path.includes('determin') || path.includes('reproduc') || path.includes('frozen'),
			),
		],
		['Editor/LSP', pathScore(paths, (path) => path.includes('mos-lsp'))],
		['Tree-sitter', pathScore(paths, (path) => path.includes('tree-sitter-mosaic'))],
		['Zed', pathScore(paths, (path) => path.includes('zed-mosaic'))],
		[
			'Testing',
			pathScore(paths, (path) => path.includes('/tests/') || path.endsWith('_test.rs') || path.endsWith('.snap')),
		],
		['Examples', pathScore(paths, (path) => path.startsWith('examples/'))],
		['CI/Release', pathScore(paths, (path) => path.startsWith('.github/') || path === 'CHANGELOG.md')],
		[
			'Docs/Tracker',
			pathScore(
				paths,
				(path) =>
					path.endsWith('.md') || path.startsWith('docs/') || path === 'README.md' || path === 'manifest-tracker.md',
			),
		],
		[
			'Project/Packages',
			pathScore(
				paths,
				(path) => path.includes('mos-packages') || path.endsWith('Cargo.toml') || path.endsWith('Cargo.lock'),
			),
		],
		[
			'Scripting/Templates',
			pathScore(paths, (path) => path.includes('template') || path.includes('script') || path.includes('style')),
		],
	];
	const best = scores.reduce((winner, candidate) => candidate[1] > winner[1] ? candidate : winner, ['Core', 0]);
	if (best[1] > 0) {
		return best[0];
	}
	if (titleKey.includes('doc')) {
		return 'Docs/Tracker';
	}
	return 'Core';
}

function chooseType(title: string, paths: ReadonlyArray<string>): string {
	const key = title.toLowerCase();
	if (key.includes('bug') || key.includes('fix') || key.includes('regression')) {
		return 'Bug';
	}
	if (key.includes('plan') || key.includes('spec')) {
		return 'Plan';
	}
	if (paths.every((path) => path.endsWith('.md') || path === 'README.md' || path === 'CHANGELOG.md')) {
		return 'Maintenance';
	}
	return 'Task';
}

function chooseSize(changedFiles: number, additions: number, deletions: number): string {
	const churn = additions + deletions;
	if (changedFiles <= 2 && churn <= 50) {
		return 'XS';
	}
	if (changedFiles <= 5 && churn <= 200) {
		return 'S';
	}
	if (changedFiles <= 12 && churn <= 800) {
		return 'M';
	}
	if (changedFiles <= 25 && churn <= 2000) {
		return 'L';
	}
	return 'XL';
}

function choosePriority(title: string, area: string): string {
	const key = title.toLowerCase();
	if (key.includes('security') || key.includes('panic') || key.includes('crash') || key.includes('ci fail')) {
		return 'P0';
	}
	if (area === 'Diagnostics' || area === 'Parser/Syntax' || area === 'Semantic/Resolver') {
		return 'P1';
	}
	return 'P2';
}

function classifyPullRequest(pr: JsonRecord): Classification {
	const paths = filePaths(pr);
	const title = stringField(pr, 'title') ?? '';
	const summary = summarizePullRequest(pr);
	const area = chooseArea(paths, title);
	const type = chooseType(title, paths);
	const size = chooseSize(summary.changedFiles, summary.additions, summary.deletions);
	const priority = choosePriority(title, area);
	const reason = `${summary.changedFiles} file(s), +${summary.additions}/-${summary.deletions}; primary paths: ${
		paths.slice(0, 5).join(', ') || 'none'
	}`;
	return { area, type, size, priority, reason };
}

function formatClassification(classification: Classification): string {
	return [
		`Area: ${classification.area}`,
		`Type: ${classification.type}`,
		`Size: ${classification.size}`,
		`Priority: ${classification.priority}`,
		'Phase: agent judgment required',
		`Phase options: ${PHASE_OPTIONS}`,
		'Phase guide:',
		...PHASE_GUIDE.map((phase) => `- ${phase}`),
		'Phase note: choose from related issues, Project 5, manifest-tracker.md, manifest.md, and shipped-code truth; do not derive from area alone. Sequence matters.',
		`Reason: ${classification.reason}`,
	].join('\n');
}

function formatPrView(pr: JsonRecord): string {
	const summary = summarizePullRequest(pr);
	const labels = labelNames(pr).join(', ') || '(none)';
	const paths = filePaths(pr);
	const body = stringField(pr, 'body') ?? '';
	return [
		`#${summary.number} ${summary.title}`,
		`URL: ${summary.url}`,
		`Author: ${summary.author}`,
		`State: ${summary.state}${summary.draft ? ' (draft)' : ''}`,
		`Base/head: ${stringField(pr, 'baseRefName') ?? ''} <- ${stringField(pr, 'headRefName') ?? ''}`,
		`Review: ${summary.reviewDecision || '(none)'}`,
		`Merge state: ${summary.mergeState || '(none)'}`,
		`Checks: ${summary.checks || '(none)'}`,
		`Churn: ${summary.changedFiles} file(s), +${summary.additions}/-${summary.deletions}`,
		`Labels: ${labels}`,
		'',
		'Files:',
		...(paths.length === 0 ? ['(none)'] : paths.slice(0, 40).map((path) => `- ${path}`)),
		'',
		body,
	].join('\n');
}

export const list_open = tool({
	description: 'List open pull requests for Mosaic with review, merge, and check signals.',
	args: {
		limit: tool.schema.number().optional().describe('Maximum PRs to list. Defaults to 20.'),
		owner: tool.schema.string().optional().describe('Repository owner. Defaults to kjanat.'),
		repository: tool.schema.string().optional().describe('Repository name. Defaults to mosaic.'),
	},
	async execute(args) {
		try {
			const repo = defaults(args.owner, args.repository);
			const limit = args.limit ?? DEFAULT_PR_LIMIT;
			const prs = (await openPullRequests(repo, limit)).map(summarizePullRequest);
			return formatPrTable(prs);
		} catch (error) {
			return `github_pr_list_open failed: ${errorMessage(error)}`;
		}
	},
});

export const view = tool({
	description: 'View one pull request with files, linked issues, and review signals.',
	args: {
		pr: tool.schema.number().describe('Pull request number to inspect.'),
		owner: tool.schema.string().optional().describe('Repository owner. Defaults to kjanat.'),
		repository: tool.schema.string().optional().describe('Repository name. Defaults to mosaic.'),
	},
	async execute(args) {
		try {
			const repo = defaults(args.owner, args.repository);
			const pr = await pullRequest(repo, args.pr);
			const issues = await relatedIssues(repo, pr);
			return `${formatPrView(pr)}\n\nRelated issues:\n${formatIssueTable(issues)}`;
		} catch (error) {
			return `github_pr_view failed: ${errorMessage(error)}`;
		}
	},
});

export const related_issues = tool({
	description: 'Find issues related to a pull request via closing refs and body mentions.',
	args: {
		pr: tool.schema.number().describe('Pull request number to inspect.'),
		owner: tool.schema.string().optional().describe('Repository owner. Defaults to kjanat.'),
		repository: tool.schema.string().optional().describe('Repository name. Defaults to mosaic.'),
	},
	async execute(args) {
		try {
			const repo = defaults(args.owner, args.repository);
			const issues = await relatedIssues(repo, await pullRequest(repo, args.pr));
			return formatIssueTable(issues);
		} catch (error) {
			return `github_pr_related_issues failed: ${errorMessage(error)}`;
		}
	},
});

export const classify = tool({
	description: 'Classify a Mosaic pull request into likely Project 5 fields from title and changed files.',
	args: {
		pr: tool.schema.number().describe('Pull request number to classify.'),
		owner: tool.schema.string().optional().describe('Repository owner. Defaults to kjanat.'),
		repository: tool.schema.string().optional().describe('Repository name. Defaults to mosaic.'),
	},
	async execute(args) {
		try {
			const repo = defaults(args.owner, args.repository);
			return formatClassification(classifyPullRequest(await pullRequest(repo, args.pr)));
		} catch (error) {
			return `github_pr_classify failed: ${errorMessage(error)}`;
		}
	},
});

export const triage = tool({
	description: 'Produce a pull-request triage summary with classification and related issues.',
	args: {
		pr: tool.schema.number().describe('Pull request number to triage.'),
		owner: tool.schema.string().optional().describe('Repository owner. Defaults to kjanat.'),
		repository: tool.schema.string().optional().describe('Repository name. Defaults to mosaic.'),
	},
	async execute(args) {
		try {
			const repo = defaults(args.owner, args.repository);
			const pr = await pullRequest(repo, args.pr);
			const summary = summarizePullRequest(pr);
			const classification = classifyPullRequest(pr);
			const issues = await relatedIssues(repo, pr);
			return [
				`#${summary.number} ${summary.title}`,
				`URL: ${summary.url}`,
				`State: ${summary.state}${summary.draft ? ' (draft)' : ''}`,
				`Review/checks: ${summary.reviewDecision || '(none)'} / ${summary.checks || '(none)'}`,
				'',
				'Classification:',
				formatClassification(classification),
				'',
				'Related issues:',
				formatIssueTable(issues),
			].join('\n');
		} catch (error) {
			return `github_pr_triage failed: ${errorMessage(error)}`;
		}
	},
});
