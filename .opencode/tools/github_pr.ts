import { graphql } from '@octokit/graphql';
import { tool } from '@opencode-ai/plugin';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, isAbsolute, join, resolve } from 'node:path';

const GRAPHQL_TIMEOUT_MS = 30_000;
const DEFAULT_PR_LIMIT = 20;
const DEFAULT_REMOTE = 'origin';
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

type ProjectItemFieldValue = {
	name: string;
	value: string;
	type: string;
};

let cachedRepository: RepositoryDefaults | null = null;

function defaults(owner: string | undefined, repository: string | undefined): RepositoryDefaults {
	const remoteRepository = repositoryFromGitConfig();
	return {
		owner: owner ?? remoteRepository.owner,
		repository: repository ?? remoteRepository.repository,
	};
}

function repositoryFromGitConfig(): RepositoryDefaults {
	if (cachedRepository !== null) {
		return cachedRepository;
	}
	const config = readFileSync(findGitConfig(process.cwd()), 'utf8');
	const remoteUrl = parseRemoteUrl(config, DEFAULT_REMOTE);
	const repository = parseGitHubRemoteUrl(remoteUrl);
	cachedRepository = repository;
	return repository;
}

function findGitConfig(start: string): string {
	let current = start;
	while (true) {
		const configPath = join(current, '.git', 'config');
		if (existsSync(configPath)) {
			return configPath;
		}

		const gitPath = join(current, '.git');
		if (existsSync(gitPath)) {
			const gitFileText = readFile(gitPath);
			const gitDir = parseGitDir(gitFileText);
			if (gitDir !== null) {
				const worktreeConfig = join(isAbsolute(gitDir) ? gitDir : resolve(current, gitDir), 'config');
				if (existsSync(worktreeConfig)) {
					return worktreeConfig;
				}
			}
		}

		const parent = dirname(current);
		if (parent === current) {
			throw new Error('Could not find .git/config');
		}
		current = parent;
	}
}

function readFile(path: string): string {
	try {
		return readFileSync(path, 'utf8');
	} catch (error) {
		return '';
	}
}

function repositoryOwnerDefault(): string {
	return repositoryFromGitConfig().owner;
}

function repositoryNameDefault(): string {
	return repositoryFromGitConfig().repository;
}

function parseGitDir(text: string): string | null {
	const prefix = 'gitdir:';
	const trimmed = text.trim();
	return trimmed.startsWith(prefix) ? trimmed.slice(prefix.length).trim() : null;
}

function parseRemoteUrl(config: string, remote: string): string {
	const section = `[remote "${remote}"]`;
	let inRemote = false;
	for (const line of config.split('\n')) {
		const trimmed = line.trim();
		if (trimmed.startsWith('[')) {
			inRemote = trimmed === section;
			continue;
		}
		if (!inRemote || !trimmed.startsWith('url')) {
			continue;
		}
		const separator = trimmed.indexOf('=');
		if (separator >= 0) {
			return trimmed.slice(separator + 1).trim();
		}
	}
	throw new Error(`Remote ${remote} URL not found in .git/config`);
}

function parseGitHubRemoteUrl(remoteUrl: string): RepositoryDefaults {
	const trimmed = remoteUrl.trim();
	const scpPrefix = 'git@github.com:';
	if (trimmed.startsWith(scpPrefix)) {
		return repositoryFromPath(trimmed.slice(scpPrefix.length));
	}

	let parsed: URL;
	try {
		parsed = new URL(trimmed);
	} catch (error) {
		throw new Error(`Unsupported GitHub remote URL: ${remoteUrl}`);
	}
	if (parsed.hostname.toLowerCase() !== 'github.com') {
		throw new Error(`Remote is not github.com: ${remoteUrl}`);
	}
	return repositoryFromPath(parsed.pathname);
}

function repositoryFromPath(path: string): RepositoryDefaults {
	const parts = path.replace(/^\/+|\/+$/g, '').split('/');
	const owner = parts[0];
	const rawRepository = parts[1];
	if (owner === undefined || owner === '' || rawRepository === undefined || rawRepository === '') {
		throw new Error(`Could not parse GitHub owner/repo from ${path}`);
	}
	const repository = rawRepository.endsWith('.git') ? rawRepository.slice(0, -4) : rawRepository;
	return { owner, repository };
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

function serialize(value: unknown): string {
	return JSON.stringify(value, null, 2) ?? 'null';
}

function success(data: JsonRecord): string {
	return serialize({ ok: true, ...data });
}

function failure(toolName: string, error: unknown): string {
	return serialize({ ok: false, tool: toolName, error: errorMessage(error) });
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

function phaseGuidance(): JsonRecord {
	return {
		required: 'agent judgment required',
		options: PHASE_OPTIONS.split(', '),
		guide: PHASE_GUIDE,
		note:
			'Choose from related issues, Project 5, manifest-tracker.md, manifest.md, and shipped-code truth; do not derive from area alone. Sequence matters.',
	};
}

function projectItemContent(item: JsonRecord): JsonRecord {
	const content = recordValue(item, 'content');
	return {
		type: content === null ? '' : stringField(content, '__typename') ?? '',
		number: content === null ? null : numberField(content, 'number'),
		title: content === null ? '' : stringField(content, 'title') ?? '',
		url: content === null ? '' : stringField(content, 'url') ?? '',
	};
}

function projectItemFieldValue(value: JsonRecord): string | null {
	const type = stringField(value, '__typename');
	if (type === 'ProjectV2ItemFieldTextValue') {
		return stringField(value, 'text');
	}
	if (type === 'ProjectV2ItemFieldSingleSelectValue') {
		return stringField(value, 'name');
	}
	if (type === 'ProjectV2ItemFieldIterationValue') {
		return stringField(value, 'title');
	}
	if (type === 'ProjectV2ItemFieldNumberValue') {
		const number = numberField(value, 'number');
		return number === null ? null : number.toString();
	}
	if (type === 'ProjectV2ItemFieldDateValue') {
		return stringField(value, 'date');
	}
	return null;
}

function projectItemFieldValues(item: JsonRecord): ReadonlyArray<ProjectItemFieldValue> {
	return nodeList(recordValue(item, 'fieldValues')).flatMap((value) => {
		const field = recordValue(value, 'field');
		const name = field === null ? null : stringField(field, 'name');
		const itemValue = projectItemFieldValue(value);
		if (name === null || itemValue === null || itemValue === '') {
			return [];
		}
		return [{ name, value: itemValue, type: stringField(value, '__typename') ?? '' }];
	});
}

function projectItems(pr: JsonRecord): ReadonlyArray<JsonRecord> {
	return nodeList(recordValue(pr, 'projectItems')).map((item) => {
		const project = recordValue(item, 'project');
		return {
			itemId: stringField(item, 'id') ?? '',
			type: stringField(item, 'type') ?? '',
			createdAt: stringField(item, 'createdAt') ?? '',
			updatedAt: stringField(item, 'updatedAt') ?? '',
			project: {
				number: project === null ? null : numberField(project, 'number'),
				title: project === null ? '' : stringField(project, 'title') ?? '',
				url: project === null ? '' : stringField(project, 'url') ?? '',
			},
			content: projectItemContent(item),
			fields: projectItemFieldValues(item),
		};
	});
}

function fileSummaries(pr: JsonRecord): ReadonlyArray<JsonRecord> {
	return nodeList(recordValue(pr, 'files')).map((file) => ({
		path: stringField(file, 'path') ?? '',
		additions: numberField(file, 'additions') ?? 0,
		deletions: numberField(file, 'deletions') ?? 0,
		changeType: stringField(file, 'changeType') ?? '',
	}));
}

function pullRequestDetails(pr: JsonRecord): JsonRecord {
	return {
		...summarizePullRequest(pr),
		baseRefName: stringField(pr, 'baseRefName') ?? '',
		headRefName: stringField(pr, 'headRefName') ?? '',
		labels: labelNames(pr),
		files: fileSummaries(pr),
		body: stringField(pr, 'body') ?? '',
	};
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
        projectItems(first: 20) {
          nodes {
            id type createdAt updatedAt
            content {
              __typename
              ... on Issue { number title url }
              ... on PullRequest { number title url }
              ... on DraftIssue { title }
            }
            project { number title url }
            fieldValues(first: 50) {
              nodes {
                __typename
                ... on ProjectV2ItemFieldTextValue {
                  text
                  field { ... on ProjectV2FieldCommon { name } }
                }
                ... on ProjectV2ItemFieldSingleSelectValue {
                  name
                  field { ... on ProjectV2FieldCommon { name } }
                }
                ... on ProjectV2ItemFieldIterationValue {
                  title
                  field { ... on ProjectV2FieldCommon { name } }
                }
                ... on ProjectV2ItemFieldNumberValue {
                  number
                  field { ... on ProjectV2FieldCommon { name } }
                }
                ... on ProjectV2ItemFieldDateValue {
                  date
                  field { ... on ProjectV2FieldCommon { name } }
                }
              }
            }
          }
        }
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

export const list_open = tool({
	description: 'List open pull requests for Mosaic with review, merge, and check signals.',
	args: {
		limit: tool.schema.number().default(DEFAULT_PR_LIMIT).describe('Maximum PRs to list.'),
		owner: tool.schema.string().default(repositoryOwnerDefault()).describe('Repository owner.'),
		repository: tool.schema.string().default(repositoryNameDefault()).describe('Repository name.'),
	},
	async execute(args) {
		try {
			const repo = await defaults(args.owner, args.repository);
			const limit = args.limit ?? DEFAULT_PR_LIMIT;
			const prs = (await openPullRequests(repo, limit)).map(summarizePullRequest);
			return success({ repository: repo, count: prs.length, pullRequests: prs });
		} catch (error) {
			return failure('github_pr_list_open', error);
		}
	},
});

export const view = tool({
	description: 'View one pull request with files, linked issues, and review signals.',
	args: {
		pr: tool.schema.number().describe('Pull request number to inspect.'),
		owner: tool.schema.string().default(repositoryOwnerDefault()).describe('Repository owner.'),
		repository: tool.schema.string().default(repositoryNameDefault()).describe('Repository name.'),
	},
	async execute(args) {
		try {
			const repo = await defaults(args.owner, args.repository);
			const pr = await pullRequest(repo, args.pr);
			const issues = await relatedIssues(repo, pr);
			return success({
				repository: repo,
				pullRequest: pullRequestDetails(pr),
				projectItems: projectItems(pr),
				relatedIssues: issues,
			});
		} catch (error) {
			return failure('github_pr_view', error);
		}
	},
});

export const related_issues = tool({
	description: 'Find issues related to a pull request via closing refs and body mentions.',
	args: {
		pr: tool.schema.number().describe('Pull request number to inspect.'),
		owner: tool.schema.string().default(repositoryOwnerDefault()).describe('Repository owner.'),
		repository: tool.schema.string().default(repositoryNameDefault()).describe('Repository name.'),
	},
	async execute(args) {
		try {
			const repo = await defaults(args.owner, args.repository);
			const issues = await relatedIssues(repo, await pullRequest(repo, args.pr));
			return success({ repository: repo, pr: args.pr, count: issues.length, relatedIssues: issues });
		} catch (error) {
			return failure('github_pr_related_issues', error);
		}
	},
});

export const classify = tool({
	description: 'Classify a Mosaic pull request into likely Project 5 fields from title and changed files.',
	args: {
		pr: tool.schema.number().describe('Pull request number to classify.'),
		owner: tool.schema.string().default(repositoryOwnerDefault()).describe('Repository owner.'),
		repository: tool.schema.string().default(repositoryNameDefault()).describe('Repository name.'),
	},
	async execute(args) {
		try {
			const repo = await defaults(args.owner, args.repository);
			return success({
				repository: repo,
				pr: args.pr,
				classification: classifyPullRequest(await pullRequest(repo, args.pr)),
				phase: phaseGuidance(),
			});
		} catch (error) {
			return failure('github_pr_classify', error);
		}
	},
});

export const triage = tool({
	description: 'Produce a pull-request triage summary with classification and related issues.',
	args: {
		pr: tool.schema.number().describe('Pull request number to triage.'),
		owner: tool.schema.string().default(repositoryOwnerDefault()).describe('Repository owner.'),
		repository: tool.schema.string().default(repositoryNameDefault()).describe('Repository name.'),
	},
	async execute(args) {
		try {
			const repo = await defaults(args.owner, args.repository);
			const pr = await pullRequest(repo, args.pr);
			const summary = summarizePullRequest(pr);
			const classification = classifyPullRequest(pr);
			const issues = await relatedIssues(repo, pr);
			return success({
				repository: repo,
				pullRequest: summary,
				projectItems: projectItems(pr),
				classification,
				phase: phaseGuidance(),
				relatedIssues: issues,
			});
		} catch (error) {
			return failure('github_pr_triage', error);
		}
	},
});
