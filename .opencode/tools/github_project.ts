import { graphql } from '@octokit/graphql';
import { tool } from '@opencode-ai/plugin';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, isAbsolute, join, resolve } from 'node:path';

const DEFAULT_PROJECT_NUMBER = 5;
const ITEM_LIMIT = 200;
const GRAPHQL_TIMEOUT_MS = 30_000;
const DEFAULT_REMOTE = 'origin';
const STATUS_OPTIONS = ['Backlog', 'Ready', 'In progress', 'In review', 'Done'] as const;
const STATUS_FILTER_OPTIONS = ['all', ...STATUS_OPTIONS] as const;
const STATUS_OPTIONAL_OPTIONS = ['none', ...STATUS_OPTIONS] as const;
const PRIORITY_OPTIONS = ['P0', 'P1', 'P2'] as const;
const PRIORITY_OPTIONAL_OPTIONS = ['none', ...PRIORITY_OPTIONS] as const;
const SIZE_OPTIONS = ['XS', 'S', 'M', 'L', 'XL'] as const;
const SIZE_OPTIONAL_OPTIONS = ['none', ...SIZE_OPTIONS] as const;
const TYPE_OPTIONS = ['Plan', 'Idea', 'Task', 'Bug', 'Maintenance'] as const;
const TYPE_OPTIONAL_OPTIONS = ['none', ...TYPE_OPTIONS] as const;
const FIELD_TYPE_OPTIONS = ['TEXT', 'NUMBER', 'DATE', 'SINGLE_SELECT', 'ITERATION'] as const;
const PHASE_OPTIONS = ['MVP 0', 'MVP 1', 'MVP 2', 'MVP 3', 'MVP 4', 'MVP 5', 'MVP 6', 'Later'] as const;
const PHASE_OPTIONAL_OPTIONS = ['none', ...PHASE_OPTIONS] as const;
const SPRINT_FILTER_OPTIONS = [
	'Sprint 1',
	'Sprint 2',
	'Sprint 3',
	'Sprint 4',
	'Sprint 5',
	'all',
	'none',
	'unscheduled',
] as const;
const SPRINT_OPTIONAL_OPTIONS = [
	'Sprint 1',
	'Sprint 2',
	'Sprint 3',
	'Sprint 4',
	'Sprint 5',
	'current',
	'next',
	'none',
] as const;
const SPRINT_SET_OPTIONS = [...SPRINT_OPTIONAL_OPTIONS, 'clear', 'unscheduled'] as const;
const SINGLE_SELECT_FIELD_OPTIONS = ['Status', 'Priority', 'Area', 'Phase', 'Type', 'Size'] as const;
const AREA_FIELD_OPTIONS = [
	'Bibliography',
	'CI/Release',
	'CLI',
	'Core',
	'Determinism',
	'Diagnostics',
	'Docs/Tracker',
	'Editor/LSP',
	'Examples',
	'Figures/Floats',
	'Fonts',
	'Incremental Cache',
	'Layout',
	'Other Backends',
	'PDF',
	'Page Reflow/Fixpoints',
	'Parser/Syntax',
	'Project/Packages',
	'Scripting/Templates',
	'Semantic/Resolver',
	'Testing',
	'Tree-sitter',
	'Zed',
] as const;
const AREA_OPTIONAL_OPTIONS = ['none', ...AREA_FIELD_OPTIONS] as const;
const AREA_OPTIONS = AREA_FIELD_OPTIONS.join(', ');
const PROJECT_FIELD_CHOICES = [
	'Area',
	'Estimate',
	'Phase',
	'Priority',
	'Size',
	'Sprint',
	'Status',
	'Type',
	'claude-code',
].join(', ');
const CLAUDE_CODE_FIELD_GUIDE =
	'`claude-code` may only contain a cloud VM hosted session URL found in user input or the issue/PR body; otherwise leave it empty. Do not use it for notes, instructions, or summaries.';
const SELECT_OPTION_GUIDE = [
	`Area: ${AREA_OPTIONS}`,
	`Phase: ${PHASE_OPTIONS.join(', ')}`,
	`Priority: ${PRIORITY_OPTIONS.join(', ')}`,
	`Size: ${SIZE_OPTIONS.join(', ')}`,
	'Estimate: hours (number)',
	`Status: ${STATUS_OPTIONS.join(', ')}`,
	`Type: ${TYPE_OPTIONS.join(', ')}`,
].join('; ');

type JsonRecord = Record<string, unknown>;

type ProjectDefaults = {
	owner: string;
	projectNumber: number;
};

type RepositoryDefaults = {
	owner: string;
	repository: string;
};

type ProjectItemSummary = {
	itemId: string;
	contentType: string;
	contentNumber: number | null;
	title: string;
	status: string;
	sprint: string;
	priority: string;
	area: string;
	url: string;
	fields: ReadonlyArray<ProjectItemFieldValue>;
};

type ProjectItemFieldValue = {
	name: string;
	value: string;
	type: string;
};

type ProjectField = {
	id: string;
	name: string;
	type: string;
	dataType: string;
	options: ReadonlyArray<ProjectOption>;
};

type ProjectOption = {
	id: string;
	name: string;
};

type CreatedIssueSummary = {
	id: string;
	number: number;
	title: string;
	url: string;
	labels: ReadonlyArray<string>;
};

type IterationField = {
	id: string;
	name: string;
	iterations: ReadonlyArray<ProjectIteration>;
};

type ProjectIteration = {
	id: string;
	title: string;
	startDate: string;
	duration: number;
};

let cachedRepository: RepositoryDefaults | null = null;

function defaults(owner: string | undefined, projectNumber: number | undefined): ProjectDefaults {
	const repository = repositoryDefaults(undefined, undefined);
	return {
		owner: owner ?? repository.owner,
		projectNumber: projectNumber ?? DEFAULT_PROJECT_NUMBER,
	};
}

function repositoryDefaults(owner: string | undefined, repository: string | undefined): RepositoryDefaults {
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

function stringValue(value: unknown): string | null {
	return typeof value === 'string' ? value : null;
}

function numberValue(value: unknown): number | null {
	return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function recordValue(record: JsonRecord, key: string): JsonRecord | null {
	const value = record[key];
	return isRecord(value) ? value : null;
}

function stringField(record: JsonRecord, key: string): string | null {
	return stringValue(record[key]);
}

function numberField(record: JsonRecord, key: string): number | null {
	return numberValue(record[key]);
}

function booleanField(record: JsonRecord, key: string): boolean | null {
	const value = record[key];
	return typeof value === 'boolean' ? value : null;
}

function arrayField(record: JsonRecord, key: string): ReadonlyArray<unknown> {
	const value = record[key];
	return Array.isArray(value) ? value : [];
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

function projectNodeFromOwnerData(data: JsonRecord): JsonRecord | null {
	const owner = recordValue(data, 'repositoryOwner');
	return owner === null ? null : recordValue(owner, 'projectV2');
}

async function projectId(project: ProjectDefaults): Promise<string> {
	const query = `query($owner: String!, $number: Int!) {
		repositoryOwner(login: $owner) {
			... on ProjectV2Owner { projectV2(number: $number) { id } }
		}
	}`;
	const data = await githubGraphql(query, { owner: project.owner, number: project.projectNumber });
	const projectNode = projectNodeFromOwnerData(data);
	const id = projectNode === null ? null : stringField(projectNode, 'id');
	if (id === null) {
		throw new Error(`Project ${project.owner}/${project.projectNumber} did not return an id`);
	}
	return id;
}

async function rawItems(project: ProjectDefaults): Promise<ReadonlyArray<JsonRecord>> {
	const query = `query($owner: String!, $number: Int!, $first: Int!, $after: String) {
		repositoryOwner(login: $owner) {
			... on ProjectV2Owner { projectV2(number: $number) { ...ProjectItems } }
		}
	}

  fragment ProjectItems on ProjectV2 {
        items(first: $first, after: $after) {
          pageInfo { hasNextPage endCursor }
          nodes {
            id
            fieldValues(first: 50) {
              nodes {
                __typename
                ... on ProjectV2ItemFieldTextValue {
                  text
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
                ... on ProjectV2ItemFieldSingleSelectValue {
                  name
                  field { ... on ProjectV2FieldCommon { name } }
                }
                ... on ProjectV2ItemFieldIterationValue {
                  title
                  startDate
                  duration
                  field { ... on ProjectV2FieldCommon { name } }
                }
              }
            }
            content {
              __typename
              ... on DraftIssue { title body }
              ... on Issue {
                number
                title
                url
                state
                body
                labels(first: 30) { nodes { name } }
              }
              ... on PullRequest {
                number
                title
                url
                state
                body
                labels(first: 30) { nodes { name } }
              }
            }
          }
        }
  }`;

	let after: string | null = null;
	const items: Array<JsonRecord> = [];
	while (items.length < ITEM_LIMIT) {
		const first = Math.min(100, ITEM_LIMIT - items.length);
		const data = await githubGraphql(query, {
			owner: project.owner,
			number: project.projectNumber,
			first,
			after,
		});
		const projectNode = projectNodeFromOwnerData(data);
		const itemConnection = projectNode === null ? null : recordValue(projectNode, 'items');
		if (itemConnection === null) {
			throw new Error(`Project ${project.owner}/${project.projectNumber} did not return items`);
		}
		items.push(...arrayField(itemConnection, 'nodes').filter(isRecord));
		const pageInfo = recordValue(itemConnection, 'pageInfo');
		if (pageInfo === null || pageInfo.hasNextPage !== true) {
			break;
		}
		after = stringField(pageInfo, 'endCursor');
		if (after === null) {
			break;
		}
	}
	return items;
}

const PROJECT_ITEM_DETAILS_FRAGMENT = `fragment ProjectItemDetails on ProjectV2Item {
  id
  project { id }
  fieldValues(first: 50) {
    nodes {
      __typename
      ... on ProjectV2ItemFieldTextValue {
        text
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
      ... on ProjectV2ItemFieldSingleSelectValue {
        name
        field { ... on ProjectV2FieldCommon { name } }
      }
      ... on ProjectV2ItemFieldIterationValue {
        title
        startDate
        duration
        field { ... on ProjectV2FieldCommon { name } }
      }
    }
  }
  content {
    __typename
    ... on DraftIssue { title body }
    ... on Issue {
      number
      title
      url
      state
      body
      labels(first: 30) { nodes { name } }
    }
    ... on PullRequest {
      number
      title
      url
      state
      body
      labels(first: 30) { nodes { name } }
    }
  }
}`;

function projectItemProjectId(item: JsonRecord): string | null {
	const project = recordValue(item, 'project');
	return project === null ? null : stringField(project, 'id');
}

function itemForProject(items: ReadonlyArray<JsonRecord>, projectIdValue: string): JsonRecord | null {
	return items.find((item) => projectItemProjectId(item) === projectIdValue) ?? null;
}

async function rawItemById(projectIdValue: string, itemId: string): Promise<JsonRecord | null> {
	const query = `query($item: ID!) {
    node(id: $item) {
      ... on ProjectV2Item { ...ProjectItemDetails }
    }
  }

  ${PROJECT_ITEM_DETAILS_FRAGMENT}`;
	const data = await githubGraphql(query, { item: itemId });
	const node = recordValue(data, 'node');
	return node !== null && projectItemProjectId(node) === projectIdValue ? node : null;
}

async function rawIssueProjectItem(
	projectIdValue: string,
	repositoryDefaults: RepositoryDefaults,
	issueNumber: number,
): Promise<JsonRecord | null> {
	const query = `query($owner: String!, $repo: String!, $number: Int!) {
    repository(owner: $owner, name: $repo) {
      issue(number: $number) {
        projectItems(first: 50) { nodes { ...ProjectItemDetails } }
      }
    }
  }

  ${PROJECT_ITEM_DETAILS_FRAGMENT}`;
	const data = await githubGraphql(query, {
		owner: repositoryDefaults.owner,
		repo: repositoryDefaults.repository,
		number: issueNumber,
	});
	const repository = recordValue(data, 'repository');
	const issue = repository === null ? null : recordValue(repository, 'issue');
	const projectItems = issue === null ? null : recordValue(issue, 'projectItems');
	const items = projectItems === null ? [] : arrayField(projectItems, 'nodes').filter(isRecord);
	return itemForProject(items, projectIdValue);
}

async function rawPullRequestProjectItem(
	projectIdValue: string,
	repositoryDefaults: RepositoryDefaults,
	prNumber: number,
): Promise<JsonRecord | null> {
	const query = `query($owner: String!, $repo: String!, $number: Int!) {
    repository(owner: $owner, name: $repo) {
      pullRequest(number: $number) {
        projectItems(first: 50) { nodes { ...ProjectItemDetails } }
      }
    }
  }

  ${PROJECT_ITEM_DETAILS_FRAGMENT}`;
	const data = await githubGraphql(query, {
		owner: repositoryDefaults.owner,
		repo: repositoryDefaults.repository,
		number: prNumber,
	});
	const repository = recordValue(data, 'repository');
	const pr = repository === null ? null : recordValue(repository, 'pullRequest');
	const projectItems = pr === null ? null : recordValue(pr, 'projectItems');
	const items = projectItems === null ? [] : arrayField(projectItems, 'nodes').filter(isRecord);
	return itemForProject(items, projectIdValue);
}

async function rawItemByTarget(project: ProjectDefaults, target: ProjectItemTarget): Promise<JsonRecord | null> {
	const id = await projectId(project);
	if (target.itemId !== '') {
		return rawItemById(id, target.itemId);
	}
	const repository = repositoryDefaults(undefined, undefined);
	if (target.issue > 0) {
		return rawIssueProjectItem(id, repository, target.issue);
	}
	if (target.pr > 0) {
		return rawPullRequestProjectItem(id, repository, target.pr);
	}
	return null;
}

async function rawFields(project: ProjectDefaults): Promise<ReadonlyArray<ProjectField>> {
	const query = `query($owner: String!, $number: Int!) {
		repositoryOwner(login: $owner) {
			... on ProjectV2Owner { projectV2(number: $number) { ...ProjectFields } }
		}
	}

  fragment ProjectFields on ProjectV2 {
        fields(first: 50) {
				nodes {
					__typename
					... on ProjectV2FieldCommon { id name }
					... on ProjectV2Field { dataType }
					... on ProjectV2SingleSelectField { id name options { id name } }
				}
        }
  }`;
	const data = await githubGraphql(query, { owner: project.owner, number: project.projectNumber });
	const projectNode = projectNodeFromOwnerData(data);
	const fields = projectNode === null ? null : recordValue(projectNode, 'fields');
	if (fields === null) {
		throw new Error(`Project ${project.owner}/${project.projectNumber} did not return fields`);
	}

	return arrayField(fields, 'nodes').filter(isRecord).flatMap((field) => {
		const id = stringField(field, 'id');
		const name = stringField(field, 'name');
		const type = stringField(field, '__typename') ?? 'ProjectV2Field';
		const dataType = stringField(field, 'dataType') ?? type;
		if (id === null || name === null) {
			return [];
		}

		const options = arrayField(field, 'options').filter(isRecord).flatMap((option) => {
			const optionId = stringField(option, 'id');
			const optionName = stringField(option, 'name');
			if (optionId === null || optionName === null) {
				return [];
			}
			return [{ id: optionId, name: optionName }];
		});

		return [{ id, name, type, dataType, options }];
	});
}

function projectFieldName(value: JsonRecord): string | null {
	const field = recordValue(value, 'field');
	return field === null ? null : stringField(field, 'name');
}

function projectFieldValue(value: JsonRecord): string | null {
	const type = stringField(value, '__typename');
	if (type === 'ProjectV2ItemFieldSingleSelectValue') {
		return stringField(value, 'name');
	}
	if (type === 'ProjectV2ItemFieldIterationValue') {
		return stringField(value, 'title');
	}
	if (type === 'ProjectV2ItemFieldTextValue') {
		return stringField(value, 'text');
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

function itemField(item: JsonRecord, names: ReadonlyArray<string>): string {
	const wanted = names.map(normalize);
	const values = recordValue(item, 'fieldValues');
	const nodes = values === null ? [] : arrayField(values, 'nodes').filter(isRecord);
	for (const node of nodes) {
		const name = projectFieldName(node);
		if (name === null || !wanted.includes(normalize(name))) {
			continue;
		}
		const value = projectFieldValue(node);
		if (value !== null) {
			return value;
		}
	}
	return '';
}

function itemFieldValues(item: JsonRecord): ReadonlyArray<ProjectItemFieldValue> {
	const values = recordValue(item, 'fieldValues');
	const nodes = values === null ? [] : arrayField(values, 'nodes').filter(isRecord);
	return nodes.flatMap((node) => {
		const name = projectFieldName(node);
		const value = projectFieldValue(node);
		if (name === null || value === null || value === '') {
			return [];
		}
		return [{ name, value, type: stringField(node, '__typename') ?? '' }];
	});
}

function contentType(content: JsonRecord | null): string {
	return content === null ? 'UNKNOWN' : stringField(content, '__typename') ?? 'UNKNOWN';
}

function contentNumber(content: JsonRecord | null): number | null {
	return content === null ? null : numberField(content, 'number');
}

function summarizeItem(item: JsonRecord): ProjectItemSummary {
	const content = recordValue(item, 'content');

	return {
		itemId: stringField(item, 'id') ?? '',
		contentType: contentType(content),
		contentNumber: contentNumber(content),
		title: (content === null ? null : stringField(content, 'title')) ?? stringField(item, 'title') ?? '',
		status: itemField(item, ['Status']),
		sprint: itemField(item, ['Sprint', 'Iteration']),
		priority: itemField(item, ['Priority']),
		area: itemField(item, ['Area']),
		url: (content === null ? null : stringField(content, 'url')) ?? '',
		fields: itemFieldValues(item),
	};
}

type ProjectItemTarget = {
	itemId: string;
	issue: number;
	pr: number;
};

type ProjectFieldUpdate = {
	field: string;
	value?: string;
	clear?: boolean;
};

function findItemByTarget(
	items: ReadonlyArray<JsonRecord>,
	target: ProjectItemTarget,
	projectNumber: number,
): ProjectItemSummary {
	const summaries = items.map(summarizeItem);
	const found = summaries.find((item) => {
		if (target.itemId !== '') {
			return item.itemId === target.itemId;
		}
		if (target.issue > 0) {
			return item.contentType === 'Issue' && item.contentNumber === target.issue;
		}
		if (target.pr > 0) {
			return item.contentType === 'PullRequest' && item.contentNumber === target.pr;
		}
		return false;
	});
	if (found === undefined) {
		const label = target.itemId !== ''
			? target.itemId
			: target.issue > 0
			? `issue #${target.issue}`
			: `PR #${target.pr}`;
		throw new Error(`${label} is not in Project ${projectNumber}`);
	}
	if (found.itemId === '') {
		throw new Error('Project item has no item id');
	}
	return found;
}

async function projectItemByTarget(project: ProjectDefaults, target: ProjectItemTarget): Promise<ProjectItemSummary> {
	try {
		return findItemByTarget(await rawItems(project), target, project.projectNumber);
	} catch (listError) {
		const item = await rawItemByTarget(project, target);
		if (item !== null) {
			return summarizeItem(item);
		}
		throw listError;
	}
}

function requireTarget(
	itemId: string | undefined,
	issue: number | undefined,
	pr: number | undefined,
): ProjectItemTarget {
	const normalized = { itemId: (itemId ?? '').trim(), issue: issue ?? 0, pr: pr ?? 0 };
	const provided = [normalized.itemId !== '', normalized.issue > 0, normalized.pr > 0].filter(Boolean).length;
	if (provided !== 1) {
		throw new Error('Provide exactly one target: itemId, issue, or pr');
	}
	return normalized;
}

function statusRank(status: string): number {
	switch (normalize(status)) {
		case 'in progress':
			return 0;
		case 'in review':
			return 1;
		case 'ready':
			return 2;
		case 'backlog':
			return 3;
		case 'done':
			return 4;
		default:
			return 5;
	}
}

function priorityRank(priority: string): number {
	switch (normalize(priority)) {
		case 'p0':
			return 0;
		case 'p1':
			return 1;
		case 'p2':
			return 2;
		default:
			return 3;
	}
}

function queueItems(
	items: ReadonlyArray<ProjectItemSummary>,
	includeBacklog: boolean | undefined,
): ReadonlyArray<ProjectItemSummary> {
	return items
		.filter((item) => normalize(item.status) !== 'done')
		.filter((item) => includeBacklog === true || normalize(item.status) !== 'backlog')
		.sort((left, right) => {
			const statusDelta = statusRank(left.status) - statusRank(right.status);
			if (statusDelta !== 0) {
				return statusDelta;
			}
			const priorityDelta = priorityRank(left.priority) - priorityRank(right.priority);
			if (priorityDelta !== 0) {
				return priorityDelta;
			}
			return (left.contentNumber ?? Number.MAX_SAFE_INTEGER) - (right.contentNumber ?? Number.MAX_SAFE_INTEGER);
		});
}

function normalize(value: string): string {
	return value.trim().toLowerCase();
}

function findField(fields: ReadonlyArray<ProjectField>, name: string): ProjectField {
	const target = normalize(name);
	const field = fields.find((candidate) => normalize(candidate.name) === target);
	if (field === undefined) {
		throw new Error(`Project field ${name} not found`);
	}
	return field;
}

function findOption(field: ProjectField, name: string): ProjectOption {
	const target = normalize(name);
	const option = field.options.find((candidate) => normalize(candidate.name) === target);
	if (option === undefined) {
		const choices = field.options.map((candidate) => candidate.name).join(', ');
		throw new Error(`Option ${name} not found for ${field.name}. Choices: ${choices}`);
	}
	return option;
}

function formatItems(items: ReadonlyArray<ProjectItemSummary>): string {
	return success({ count: items.length, items });
}

function filterItems(
	items: ReadonlyArray<ProjectItemSummary>,
	status: string | undefined,
	sprint: string | undefined,
	includeDone: boolean | undefined,
): ReadonlyArray<ProjectItemSummary> {
	const statusFilter = status === undefined || status === 'all' ? null : normalize(status);
	const sprintFilter = sprint === undefined || sprint === 'all' ? null : normalize(sprint);
	return items.filter((item) => {
		if (includeDone !== true && statusFilter !== 'done' && normalize(item.status) === 'done') {
			return false;
		}
		if (statusFilter !== null && normalize(item.status) !== statusFilter) {
			return false;
		}
		if (sprintFilter !== null) {
			if ((sprintFilter === 'none' || sprintFilter === 'unscheduled') && item.sprint === '') {
				return true;
			}
			return normalize(item.sprint) === sprintFilter;
		}
		return true;
	});
}

async function issueNodeId(repositoryDefaults: RepositoryDefaults, issueNumber: number): Promise<string> {
	const query = `query($owner: String!, $name: String!, $number: Int!) {
    repository(owner: $owner, name: $name) {
      issue(number: $number) { id }
    }
  }`;
	const data = await githubGraphql(query, {
		owner: repositoryDefaults.owner,
		name: repositoryDefaults.repository,
		number: issueNumber,
	});
	const repository = recordValue(data, 'repository');
	const issue = repository === null ? null : recordValue(repository, 'issue');
	const id = issue === null ? null : stringField(issue, 'id');
	if (id === null) {
		throw new Error(`Issue #${issueNumber} not found in ${repositoryDefaults.owner}/${repositoryDefaults.repository}`);
	}
	return id;
}

async function iterationField(project: ProjectDefaults): Promise<IterationField> {
	const query = `query($owner: String!, $number: Int!) {
		repositoryOwner(login: $owner) {
			... on ProjectV2Owner { projectV2(number: $number) { ...ProjectIterations } }
		}
	}

  fragment ProjectIterations on ProjectV2 {
        fields(first: 50) {
          nodes {
            __typename
            ... on ProjectV2IterationField {
              id
              name
              configuration {
                iterations { id title startDate duration }
                completedIterations { id title startDate duration }
              }
            }
          }
        }
  }`;

	const data = await githubGraphql(query, { owner: project.owner, number: project.projectNumber });
	const projectNode = projectNodeFromOwnerData(data);
	const fields = projectNode === null ? null : recordValue(projectNode, 'fields');
	const nodes = fields === null ? [] : arrayField(fields, 'nodes');

	for (const candidate of nodes.filter(isRecord)) {
		if (stringField(candidate, '__typename') !== 'ProjectV2IterationField') {
			continue;
		}
		const fieldId = stringField(candidate, 'id');
		const name = stringField(candidate, 'name');
		const configuration = recordValue(candidate, 'configuration');
		if (fieldId === null || name === null || configuration === null) {
			continue;
		}
		const active = arrayField(configuration, 'iterations').filter(isRecord).flatMap(readIteration);
		const completed = arrayField(configuration, 'completedIterations').filter(isRecord).flatMap(readIteration);
		return { id: fieldId, name, iterations: [...completed, ...active] };
	}

	throw new Error('Project iteration/sprint field not found');
}

function readIteration(record: JsonRecord): ReadonlyArray<ProjectIteration> {
	const id = stringField(record, 'id');
	const title = stringField(record, 'title');
	const startDate = stringField(record, 'startDate');
	const duration = numberField(record, 'duration');
	if (id === null || title === null || startDate === null || duration === null) {
		return [];
	}
	return [{ id, title, startDate, duration }];
}

function dateAtUtcMidnight(date: string): Date {
	return new Date(`${date}T00:00:00.000Z`);
}

function resolveCurrentIteration(iterations: ReadonlyArray<ProjectIteration>, now: Date): ProjectIteration | null {
	return iterations.find((iteration) => {
		const start = dateAtUtcMidnight(iteration.startDate);
		const end = new Date(start.getTime() + iteration.duration * 24 * 60 * 60 * 1000);
		return now >= start && now < end;
	}) ?? null;
}

function resolveNextIteration(iterations: ReadonlyArray<ProjectIteration>, now: Date): ProjectIteration | null {
	const future = iterations
		.filter((iteration) => dateAtUtcMidnight(iteration.startDate) > now)
		.sort((left, right) => dateAtUtcMidnight(left.startDate).getTime() - dateAtUtcMidnight(right.startDate).getTime());
	return future[0] ?? null;
}

function resolveIteration(iterations: ReadonlyArray<ProjectIteration>, sprint: string): ProjectIteration | null {
	const key = normalize(sprint);
	if (key === 'current') {
		return resolveCurrentIteration(iterations, new Date());
	}
	if (key === 'next') {
		return resolveNextIteration(iterations, new Date());
	}
	return iterations.find((iteration) => normalize(iteration.title) === key || normalize(iteration.id) === key) ?? null;
}

async function setSingleSelectField(
	projectIdValue: string,
	itemId: string,
	fieldId: string,
	optionId: string,
): Promise<void> {
	const mutation = `mutation($project: ID!, $item: ID!, $field: ID!, $option: String!) {
    updateProjectV2ItemFieldValue(
      input: {
        projectId: $project
        itemId: $item
        fieldId: $field
        value: { singleSelectOptionId: $option }
      }
    ) {
      projectV2Item { id }
    }
  }`;
	await githubGraphql(mutation, { project: projectIdValue, item: itemId, field: fieldId, option: optionId });
}

async function setNumberField(projectIdValue: string, itemId: string, fieldId: string, value: number): Promise<void> {
	const mutation = `mutation($project: ID!, $item: ID!, $field: ID!, $value: Float!) {
    updateProjectV2ItemFieldValue(
      input: {
        projectId: $project
        itemId: $item
        fieldId: $field
        value: { number: $value }
      }
    ) {
      projectV2Item { id }
    }
  }`;
	await githubGraphql(mutation, { project: projectIdValue, item: itemId, field: fieldId, value });
}

async function setTextField(projectIdValue: string, itemId: string, fieldId: string, value: string): Promise<void> {
	const mutation = `mutation($project: ID!, $item: ID!, $field: ID!, $value: String!) {
    updateProjectV2ItemFieldValue(
      input: {
        projectId: $project
        itemId: $item
        fieldId: $field
        value: { text: $value }
      }
    ) {
      projectV2Item { id }
    }
  }`;
	await githubGraphql(mutation, { project: projectIdValue, item: itemId, field: fieldId, value });
}

async function setDateField(projectIdValue: string, itemId: string, fieldId: string, value: string): Promise<void> {
	const mutation = `mutation($project: ID!, $item: ID!, $field: ID!, $value: Date!) {
    updateProjectV2ItemFieldValue(
      input: {
        projectId: $project
        itemId: $item
        fieldId: $field
        value: { date: $value }
      }
    ) {
      projectV2Item { id }
    }
  }`;
	await githubGraphql(mutation, { project: projectIdValue, item: itemId, field: fieldId, value });
}

async function setIterationField(
	projectIdValue: string,
	itemId: string,
	fieldId: string,
	iterationId: string,
): Promise<void> {
	const mutation = `mutation($project: ID!, $item: ID!, $field: ID!, $iteration: String!) {
    updateProjectV2ItemFieldValue(
      input: {
        projectId: $project
        itemId: $item
        fieldId: $field
        value: { iterationId: $iteration }
      }
    ) {
      projectV2Item { id }
    }
  }`;
	await githubGraphql(mutation, { project: projectIdValue, item: itemId, field: fieldId, iteration: iterationId });
}

async function clearProjectField(projectIdValue: string, itemId: string, fieldId: string): Promise<void> {
	const mutation = `mutation($project: ID!, $item: ID!, $field: ID!) {
    clearProjectV2ItemFieldValue(
      input: { projectId: $project, itemId: $item, fieldId: $field }
    ) {
      projectV2Item { id }
    }
  }`;
	await githubGraphql(mutation, { project: projectIdValue, item: itemId, field: fieldId });
}

async function addIssueItem(projectIdValue: string, issueId: string): Promise<string> {
	const mutation = `mutation($project: ID!, $content: ID!) {
    addProjectV2ItemById(input: { projectId: $project, contentId: $content }) {
      item { id }
    }
  }`;
	const data = await githubGraphql(mutation, { project: projectIdValue, content: issueId });
	const payload = recordValue(data, 'addProjectV2ItemById');
	const item = payload === null ? null : recordValue(payload, 'item');
	const id = item === null ? null : stringField(item, 'id');
	if (id === null) {
		throw new Error('addProjectV2ItemById did not return an item id');
	}
	return id;
}

async function repositoryNodeId(repositoryDefaults: RepositoryDefaults): Promise<string> {
	const query = `query($owner: String!, $name: String!) {
    repository(owner: $owner, name: $name) { id }
  }`;
	const data = await githubGraphql(query, {
		owner: repositoryDefaults.owner,
		name: repositoryDefaults.repository,
	});
	const repository = recordValue(data, 'repository');
	const id = repository === null ? null : stringField(repository, 'id');
	if (id === null) {
		throw new Error(`Repository ${repositoryDefaults.owner}/${repositoryDefaults.repository} did not return an id`);
	}
	return id;
}

async function labelNodeId(repositoryDefaults: RepositoryDefaults, label: string): Promise<string> {
	const query = `query($owner: String!, $name: String!, $label: String!) {
    repository(owner: $owner, name: $name) { label(name: $label) { id } }
  }`;
	const data = await githubGraphql(query, {
		owner: repositoryDefaults.owner,
		name: repositoryDefaults.repository,
		label,
	});
	const repository = recordValue(data, 'repository');
	const labelNode = repository === null ? null : recordValue(repository, 'label');
	const id = labelNode === null ? null : stringField(labelNode, 'id');
	if (id === null) {
		throw new Error(`Label ${label} not found in ${repositoryDefaults.owner}/${repositoryDefaults.repository}`);
	}
	return id;
}

async function labelNodeIds(
	repositoryDefaults: RepositoryDefaults,
	labels: ReadonlyArray<string>,
): Promise<ReadonlyArray<string>> {
	return Promise.all(labels.map((label) => labelNodeId(repositoryDefaults, label)));
}

function issueLabelNames(issue: JsonRecord): ReadonlyArray<string> {
	const labels = recordValue(issue, 'labels');
	const nodes = labels === null ? [] : arrayField(labels, 'nodes').filter(isRecord);
	return nodes.flatMap((node) => {
		const name = stringField(node, 'name');
		return name === null ? [] : [name];
	});
}

async function createRepositoryIssue(
	repositoryDefaults: RepositoryDefaults,
	title: string,
	body: string,
	labels: ReadonlyArray<string>,
): Promise<CreatedIssueSummary> {
	const normalizedTitle = title.trim();
	if (normalizedTitle === '') {
		throw new Error('Issue title must be non-empty');
	}
	const [repositoryId, labelIds] = await Promise.all([
		repositoryNodeId(repositoryDefaults),
		labelNodeIds(repositoryDefaults, labels),
	]);
	const mutation = `mutation($repository: ID!, $title: String!, $body: String!, $labelIds: [ID!]) {
    createIssue(input: { repositoryId: $repository, title: $title, body: $body, labelIds: $labelIds }) {
      issue {
        id
        number
        title
        url
        labels(first: 30) { nodes { name } }
      }
    }
  }`;
	const data = await githubGraphql(mutation, {
		repository: repositoryId,
		title: normalizedTitle,
		body,
		labelIds,
	});
	const payload = recordValue(data, 'createIssue');
	const issue = payload === null ? null : recordValue(payload, 'issue');
	if (issue === null) {
		throw new Error('createIssue did not return an issue');
	}
	const id = stringField(issue, 'id');
	const number = numberField(issue, 'number');
	const createdTitle = stringField(issue, 'title');
	const url = stringField(issue, 'url');
	if (id === null || number === null || createdTitle === null || url === null) {
		throw new Error('createIssue did not return issue id, number, title, and url');
	}
	return { id, number, title: createdTitle, url, labels: issueLabelNames(issue) };
}

function parseNumber(value: string): number {
	const parsed = Number.parseFloat(value);
	if (!Number.isFinite(parsed)) {
		throw new Error(`Expected number field value, got ${value}`);
	}
	return parsed;
}

async function setDiscoveredField(
	project: ProjectDefaults,
	projectIdValue: string,
	itemId: string,
	field: ProjectField,
	value: string,
): Promise<string> {
	if (field.type === 'ProjectV2SingleSelectField' || field.dataType === 'SINGLE_SELECT') {
		const option = findOption(field, value);
		await setSingleSelectField(projectIdValue, itemId, field.id, option.id);
		return option.name;
	}
	if (field.type === 'ProjectV2IterationField' || field.dataType === 'ITERATION') {
		const iteration = resolveIteration(
			await iterationField(project).then((item) => item.iterations),
			value,
		);
		if (iteration === null) {
			throw new Error(`Sprint ${value} not found`);
		}
		await setIterationField(projectIdValue, itemId, field.id, iteration.id);
		return iteration.title;
	}
	if (field.dataType === 'NUMBER') {
		const parsed = parseNumber(value);
		await setNumberField(projectIdValue, itemId, field.id, parsed);
		return parsed.toString();
	}
	if (field.dataType === 'DATE') {
		await setDateField(projectIdValue, itemId, field.id, value);
		return value;
	}
	if (field.dataType === 'TEXT') {
		await setTextField(projectIdValue, itemId, field.id, value);
		return value;
	}
	throw new Error(
		`Field ${field.name} has unsupported type ${field.dataType}; use issue/PR-native mutations for built-ins`,
	);
}

function parseCsv(value: string | undefined): ReadonlyArray<string> {
	if (value === undefined || value.trim() === '') {
		return [];
	}
	return value.split(',').map((item) => item.trim()).filter((item) => item !== '');
}

async function createProjectField(
	projectIdValue: string,
	name: string,
	dataType: string,
	optionsCsv: string | undefined,
): Promise<string> {
	const normalizedType = dataType.trim().toUpperCase().replace(/-/g, '_');
	const options = parseCsv(optionsCsv).map((option) => ({ name: option, color: 'GRAY', description: '' }));
	const mutation =
		`mutation($project: ID!, $name: String!, $dataType: ProjectV2CustomFieldType!, $options: [ProjectV2SingleSelectFieldOptionInput!]) {
    createProjectV2Field(input: { projectId: $project, name: $name, dataType: $dataType, singleSelectOptions: $options }) {
      projectV2Field { ... on ProjectV2FieldCommon { id name } }
    }
  }`;
	const variables: JsonRecord = { project: projectIdValue, name, dataType: normalizedType };
	if (normalizedType === 'SINGLE_SELECT') {
		variables.options = options;
	} else if (options.length > 0) {
		throw new Error('Options are only valid for SINGLE_SELECT fields');
	}
	const data = await githubGraphql(mutation, variables);
	const payload = recordValue(data, 'createProjectV2Field');
	const field = payload === null ? null : recordValue(payload, 'projectV2Field');
	const id = field === null ? null : stringField(field, 'id');
	if (id === null) {
		throw new Error('createProjectV2Field did not return a field id');
	}
	return id;
}

async function setProjectFieldForTarget(
	project: ProjectDefaults,
	target: ProjectItemTarget,
	fieldName: string,
	value: string | undefined,
	clear: boolean | undefined,
): Promise<JsonRecord> {
	const [id, item, projectFields] = await Promise.all([
		projectId(project),
		projectItemByTarget(project, target),
		rawFields(project),
	]);
	const field = findField(projectFields, fieldName);
	if (clear === true) {
		await clearProjectField(id, item.itemId, field.id);
		return { project, item, field: field.name, cleared: true };
	}
	if (value === undefined) {
		throw new Error(`Value required for ${field.name}; pass clear=true to clear it`);
	}
	const appliedValue = await setDiscoveredField(project, id, item.itemId, field, value);
	return { project, item, field: field.name, value: appliedValue, cleared: false };
}

function parseFieldUpdates(rawUpdates: string): ReadonlyArray<ProjectFieldUpdate> {
	let parsed: unknown;
	try {
		parsed = JSON.parse(rawUpdates);
	} catch (error) {
		throw new Error(`updates must be a JSON array: ${errorMessage(error)}`);
	}
	if (!Array.isArray(parsed)) {
		throw new Error('updates must be a JSON array');
	}
	return parsed.map((item, index) => {
		if (!isRecord(item)) {
			throw new Error(`updates[${index}] must be an object`);
		}
		const field = stringField(item, 'field');
		if (field === null || field.trim() === '') {
			throw new Error(`updates[${index}].field must be a non-empty string`);
		}
		const value = stringField(item, 'value');
		const clear = booleanField(item, 'clear') ?? false;
		if (value === null && !clear) {
			throw new Error(`updates[${index}] must include value or clear=true`);
		}
		return { field, value: value === null || value === '' ? undefined : value, clear };
	});
}

async function setProjectFieldsForTarget(
	project: ProjectDefaults,
	target: ProjectItemTarget,
	updates: ReadonlyArray<ProjectFieldUpdate>,
): Promise<JsonRecord> {
	if (updates.length === 0) {
		throw new Error('updates must include at least one field update');
	}
	const [id, item, projectFields] = await Promise.all([
		projectId(project),
		projectItemByTarget(project, target),
		rawFields(project),
	]);

	const results: Array<JsonRecord> = [];
	for (const update of updates) {
		const field = findField(projectFields, update.field);
		if (update.clear === true) {
			await clearProjectField(id, item.itemId, field.id);
			results.push({ field: field.name, cleared: true });
			continue;
		}
		if (update.value === undefined) {
			throw new Error(`Value required for ${field.name}; pass clear=true to clear it`);
		}
		const value = await setDiscoveredField(project, id, item.itemId, field, update.value);
		results.push({ field: field.name, value, cleared: false });
	}

	const updatedItem = await projectItemByTarget(project, target);
	return { project, item: updatedItem, updates: results };
}

function formatFields(fields: ReadonlyArray<ProjectField>, iteration: IterationField | null): string {
	return success({
		fields: fields.map((field) => ({
			...field,
			options: field.options,
			iterations: iteration !== null && normalize(field.name) === normalize(iteration.name) ? iteration.iterations : [],
		})),
	});
}

export const list = tool({
	description: 'Read-only: list GitHub Project 5 items for Mosaic, with optional status/sprint filters.',
	args: {
		status: tool.schema.enum(STATUS_FILTER_OPTIONS).default('all').describe('Optional status filter.'),
		sprint: tool.schema.enum(SPRINT_FILTER_OPTIONS).default('all').describe('Optional sprint title filter.'),
		includeDone: tool.schema.boolean().default(false).describe('Include Done items.'),
		owner: tool.schema.string().default(repositoryOwnerDefault()).describe('GitHub Project owner.'),
		projectNumber: tool.schema.number().default(DEFAULT_PROJECT_NUMBER).describe('GitHub Project number.'),
	},
	async execute(args) {
		try {
			const project = defaults(args.owner, args.projectNumber);
			const items = (await rawItems(project)).map(summarizeItem);
			return formatItems(filterItems(items, args.status, args.sprint, args.includeDone));
		} catch (error) {
			return failure('github_project_list', error);
		}
	},
});

export const queue = tool({
	description: 'Read-only: show the active Mosaic Project 5 queue, sorted by status and priority.',
	args: {
		includeBacklog: tool.schema.boolean().default(false).describe('Include Backlog items.'),
		owner: tool.schema.string().default(repositoryOwnerDefault()).describe('GitHub Project owner.'),
		projectNumber: tool.schema.number().default(DEFAULT_PROJECT_NUMBER).describe('GitHub Project number.'),
	},
	async execute(args) {
		try {
			const project = defaults(args.owner, args.projectNumber);
			const items = (await rawItems(project)).map(summarizeItem);
			return formatItems(queueItems(items, args.includeBacklog));
		} catch (error) {
			return failure('github_project_queue', error);
		}
	},
});

export const fields = tool({
	description: 'Read-only: list Mosaic GitHub Project 5 fields, options, and sprint choices.',
	args: {
		owner: tool.schema.string().default(repositoryOwnerDefault()).describe('GitHub Project owner.'),
		projectNumber: tool.schema.number().default(DEFAULT_PROJECT_NUMBER).describe('GitHub Project number.'),
	},
	async execute(args) {
		try {
			const project = defaults(args.owner, args.projectNumber);
			const [projectFields, sprintField] = await Promise.all([
				rawFields(project),
				iterationField(project).catch(() => null),
			]);
			return formatFields(projectFields, sprintField);
		} catch (error) {
			return failure('github_project_fields', error);
		}
	},
});

export const view = tool({
	description: 'Read-only: view one GitHub Project 5 item by issue, PR, or item id.',
	args: {
		issue: tool.schema.number().default(0).describe('Issue number to inspect. Use 0 when targeting PR or itemId.'),
		pr: tool.schema.number().default(0).describe(
			'Pull request number to inspect. Use 0 when targeting issue or itemId.',
		),
		itemId: tool.schema.string().default('').describe(
			'Project item id to inspect. Use empty string when targeting issue or PR.',
		),
		owner: tool.schema.string().default(repositoryOwnerDefault()).describe('GitHub Project owner.'),
		projectNumber: tool.schema.number().default(DEFAULT_PROJECT_NUMBER).describe('GitHub Project number.'),
	},
	async execute(args) {
		try {
			const project = defaults(args.owner, args.projectNumber);
			const target = requireTarget(args.itemId, args.issue, args.pr);
			const item = await projectItemByTarget(project, target);
			return success({ project, item });
		} catch (error) {
			return failure('github_project_view', error);
		}
	},
});

export const add_issue = tool({
	description: 'Mutates Project 5: add an existing Mosaic GitHub issue, optionally setting status and sprint.',
	args: {
		issue: tool.schema.number().describe('Issue number to add to Project 5.'),
		status: tool.schema.enum(STATUS_OPTIONAL_OPTIONS).default('none').describe('Optional status to set.'),
		sprint: tool.schema.enum(SPRINT_OPTIONAL_OPTIONS).default('none').describe(
			'Optional sprint title/id, current, or next.',
		),
		owner: tool.schema.string().default(repositoryOwnerDefault()).describe('GitHub Project owner.'),
		projectNumber: tool.schema.number().default(DEFAULT_PROJECT_NUMBER).describe('GitHub Project number.'),
	},
	async execute(args) {
		try {
			const project = defaults(args.owner, args.projectNumber);
			const repository = repositoryDefaults(undefined, undefined);
			const id = await projectId(project);
			const itemId = await addIssueItem(id, await issueNodeId(repository, args.issue));
			const updates: Array<JsonRecord> = [{ field: 'item', value: itemId }];

			if (args.status !== 'none') {
				const statusField = findField(await rawFields(project), 'Status');
				const option = findOption(statusField, args.status);
				await setSingleSelectField(id, itemId, statusField.id, option.id);
				updates.push({ field: statusField.name, value: option.name });
			}

			if (args.sprint !== 'none') {
				const field = await iterationField(project);
				const iteration = resolveIteration(field.iterations, args.sprint);
				if (iteration === null) {
					const choices = field.iterations.map((candidate) => candidate.title).join(', ');
					throw new Error(`Sprint ${args.sprint} not found. Choices: current, next, ${choices}`);
				}
				await setIterationField(id, itemId, field.id, iteration.id);
				updates.push({ field: field.name, value: iteration.title });
			}

			return success({ project, repository, issue: args.issue, itemId, updates });
		} catch (error) {
			return failure('github_project_add_issue', error);
		}
	},
});

export const create_issue = tool({
	description: 'Mutates GitHub and Project 5: create a Mosaic issue, add it to Project 5, and set planning fields.',
	args: {
		title: tool.schema.string().describe('Issue title.'),
		body: tool.schema.string().default('').describe('Issue body in Markdown.'),
		labels: tool.schema.string().default('').describe('Comma-separated repository label names to apply.'),
		status: tool.schema.enum(STATUS_OPTIONAL_OPTIONS).default('none').describe('Optional project Status to set.'),
		sprint: tool.schema.enum(SPRINT_OPTIONAL_OPTIONS).default('none').describe(
			'Optional project Sprint title/id, current, or next.',
		),
		priority: tool.schema.enum(PRIORITY_OPTIONAL_OPTIONS).default('none').describe('Optional project Priority.'),
		size: tool.schema.enum(SIZE_OPTIONAL_OPTIONS).default('none').describe('Optional project Size.'),
		estimate: tool.schema.number().default(0).describe('Optional project Estimate, measured in hours. 0 means unset.'),
		phase: tool.schema.enum(PHASE_OPTIONAL_OPTIONS).default('none').describe('Optional project Phase.'),
		area: tool.schema.enum(AREA_OPTIONAL_OPTIONS).default('none').describe('Optional project Area.'),
		type: tool.schema.enum(TYPE_OPTIONAL_OPTIONS).default('none').describe('Optional project Type.'),
		claudeCode: tool.schema.string().default('').describe(CLAUDE_CODE_FIELD_GUIDE),
		repository: tool.schema.string().default(repositoryNameDefault()).describe('Repository name to create issue in.'),
		owner: tool.schema.string().default(repositoryOwnerDefault()).describe('GitHub Project owner.'),
		projectNumber: tool.schema.number().default(DEFAULT_PROJECT_NUMBER).describe('GitHub Project number.'),
	},
	async execute(args) {
		try {
			const project = defaults(args.owner, args.projectNumber);
			const repository = repositoryDefaults(undefined, args.repository);
			const issue = await createRepositoryIssue(repository, args.title, args.body, parseCsv(args.labels));
			const projectIdValue = await projectId(project);
			const itemId = await addIssueItem(projectIdValue, issue.id);
			const fieldUpdates: Array<ProjectFieldUpdate> = [];

			if (args.status !== 'none') {
				fieldUpdates.push({ field: 'Status', value: args.status });
			}
			if (args.sprint !== 'none') {
				fieldUpdates.push({ field: 'Sprint', value: args.sprint });
			}
			if (args.priority !== 'none') {
				fieldUpdates.push({ field: 'Priority', value: args.priority });
			}
			if (args.size !== 'none') {
				fieldUpdates.push({ field: 'Size', value: args.size });
			}
			if (args.estimate > 0) {
				fieldUpdates.push({ field: 'Estimate', value: args.estimate.toString() });
			}
			if (args.phase !== 'none') {
				fieldUpdates.push({ field: 'Phase', value: args.phase });
			}
			if (args.area !== 'none') {
				fieldUpdates.push({ field: 'Area', value: args.area });
			}
			if (args.type !== 'none') {
				fieldUpdates.push({ field: 'Type', value: args.type });
			}
			if (args.claudeCode.trim() !== '') {
				fieldUpdates.push({ field: 'claude-code', value: args.claudeCode });
			}

			const target = { itemId, issue: 0, pr: 0 };
			let item: unknown;
			let appliedUpdates: ReadonlyArray<JsonRecord> = [];
			if (fieldUpdates.length === 0) {
				item = await projectItemByTarget(project, target);
			} else {
				const fieldResult = await setProjectFieldsForTarget(project, target, fieldUpdates);
				item = fieldResult.item;
				appliedUpdates = arrayField(fieldResult, 'updates').filter(isRecord);
			}

			return success({
				project,
				repository,
				issue,
				itemId,
				item,
				updates: [{ field: 'item', value: itemId }, ...appliedUpdates],
			});
		} catch (error) {
			return failure('github_project_create_issue', error);
		}
	},
});

export const set_field = tool({
	description: 'Low-level mutating setter: set or clear any supported GitHub Project 5 field by issue, PR, or item id.',
	args: {
		issue: tool.schema.number().default(0).describe('Issue number to update. Use 0 when targeting PR or itemId.'),
		pr: tool.schema.number().default(0).describe(
			'Pull request number to update. Use 0 when targeting issue or itemId.',
		),
		itemId: tool.schema.string().default('').describe(
			'Project item id to update. Use empty string when targeting issue or PR.',
		),
		field: tool.schema.string().describe(`Project field name. Known fields: ${PROJECT_FIELD_CHOICES}.`),
		value: tool.schema.string().default('').describe(
			`Value to set. Use empty string only when clear is true. Select guides: ${SELECT_OPTION_GUIDE}. ${CLAUDE_CODE_FIELD_GUIDE}`,
		),
		clear: tool.schema.boolean().default(false).describe('Clear the field instead of setting a value.'),
		owner: tool.schema.string().default(repositoryOwnerDefault()).describe('GitHub Project owner.'),
		projectNumber: tool.schema.number().default(DEFAULT_PROJECT_NUMBER).describe('GitHub Project number.'),
	},
	async execute(args) {
		try {
			const project = defaults(args.owner, args.projectNumber);
			const target = requireTarget(args.itemId, args.issue, args.pr);
			return success(
				await setProjectFieldForTarget(
					project,
					target,
					args.field,
					args.value === '' ? undefined : args.value,
					args.clear,
				),
			);
		} catch (error) {
			return failure('github_project_set_field', error);
		}
	},
});

export const set_fields = tool({
	description:
		'Low-level mutating batch setter: set or clear multiple GitHub Project 5 fields on one issue, PR, or item id.',
	args: {
		issue: tool.schema.number().default(0).describe('Issue number to update. Use 0 when targeting PR or itemId.'),
		pr: tool.schema.number().default(0).describe(
			'Pull request number to update. Use 0 when targeting issue or itemId.',
		),
		itemId: tool.schema.string().default('').describe(
			'Project item id to update. Use empty string when targeting issue or PR.',
		),
		updates: tool.schema.string().describe(
			`JSON array of updates, e.g. [{"field":"Priority","value":"P1"},{"field":"Sprint","value":"Sprint 2"}]. Known fields: ${PROJECT_FIELD_CHOICES}. Select guides: ${SELECT_OPTION_GUIDE}. ${CLAUDE_CODE_FIELD_GUIDE} Use {"field":"Sprint","clear":true} to clear.`,
		),
		owner: tool.schema.string().default(repositoryOwnerDefault()).describe('GitHub Project owner.'),
		projectNumber: tool.schema.number().default(DEFAULT_PROJECT_NUMBER).describe('GitHub Project number.'),
	},
	async execute(args) {
		try {
			const project = defaults(args.owner, args.projectNumber);
			const target = requireTarget(args.itemId, args.issue, args.pr);
			return success(await setProjectFieldsForTarget(project, target, parseFieldUpdates(args.updates)));
		} catch (error) {
			return failure('github_project_set_fields', error);
		}
	},
});

export const create_field = tool({
	description: 'Admin mutation: create a GitHub Project 5 custom field. Use only when explicitly requested.',
	args: {
		name: tool.schema.string().describe('New field name.'),
		dataType: tool.schema.enum(FIELD_TYPE_OPTIONS).describe('Field type.'),
		options: tool.schema.string().default('').describe('Comma-separated options for SINGLE_SELECT fields.'),
		owner: tool.schema.string().default(repositoryOwnerDefault()).describe('GitHub Project owner.'),
		projectNumber: tool.schema.number().default(DEFAULT_PROJECT_NUMBER).describe('GitHub Project number.'),
	},
	async execute(args) {
		try {
			const project = defaults(args.owner, args.projectNumber);
			const id = await createProjectField(await projectId(project), args.name, args.dataType, args.options);
			return success({ project, field: { id, name: args.name, dataType: args.dataType } });
		} catch (error) {
			return failure('github_project_create_field', error);
		}
	},
});

export const set_status = tool({
	description: 'Mutates Project 5: set the Status field for a Mosaic item.',
	args: {
		issue: tool.schema.number().default(0).describe('Issue number to update. Use 0 when targeting PR or itemId.'),
		pr: tool.schema.number().default(0).describe(
			'Pull request number to update. Use 0 when targeting issue or itemId.',
		),
		itemId: tool.schema.string().default('').describe(
			'Project item id to update. Use empty string when targeting issue or PR.',
		),
		status: tool.schema.enum(STATUS_OPTIONS).describe('Status name.'),
		owner: tool.schema.string().default(repositoryOwnerDefault()).describe('GitHub Project owner.'),
		projectNumber: tool.schema.number().default(DEFAULT_PROJECT_NUMBER).describe('GitHub Project number.'),
	},
	async execute(args) {
		try {
			const project = defaults(args.owner, args.projectNumber);
			const target = requireTarget(args.itemId, args.issue, args.pr);
			return success(await setProjectFieldForTarget(project, target, 'Status', args.status, false));
		} catch (error) {
			return failure('github_project_set_status', error);
		}
	},
});

export const set_select = tool({
	description: 'Mutates Project 5: set any supported single-select field for a Mosaic item.',
	args: {
		issue: tool.schema.number().default(0).describe('Issue number to update. Use 0 when targeting PR or itemId.'),
		pr: tool.schema.number().default(0).describe(
			'Pull request number to update. Use 0 when targeting issue or itemId.',
		),
		itemId: tool.schema.string().default('').describe(
			'Project item id to update. Use empty string when targeting issue or PR.',
		),
		field: tool.schema.enum(SINGLE_SELECT_FIELD_OPTIONS).describe('Single-select field name.'),
		option: tool.schema.string().describe(`Option name to set. Choices by field: ${SELECT_OPTION_GUIDE}.`),
		owner: tool.schema.string().default(repositoryOwnerDefault()).describe('GitHub Project owner.'),
		projectNumber: tool.schema.number().default(DEFAULT_PROJECT_NUMBER).describe('GitHub Project number.'),
	},
	async execute(args) {
		try {
			const project = defaults(args.owner, args.projectNumber);
			const target = requireTarget(args.itemId, args.issue, args.pr);
			return success(await setProjectFieldForTarget(project, target, args.field, args.option, false));
		} catch (error) {
			return failure('github_project_set_select', error);
		}
	},
});

export const set_sprint = tool({
	description: 'Mutates Project 5: set or clear the Sprint/Iteration field for a Mosaic item.',
	args: {
		issue: tool.schema.number().default(0).describe('Issue number to update. Use 0 when targeting PR or itemId.'),
		pr: tool.schema.number().default(0).describe(
			'Pull request number to update. Use 0 when targeting issue or itemId.',
		),
		itemId: tool.schema.string().default('').describe(
			'Project item id to update. Use empty string when targeting issue or PR.',
		),
		sprint: tool.schema.enum(SPRINT_SET_OPTIONS).describe('Sprint title/id, current, next, or clear/none/unscheduled.'),
		owner: tool.schema.string().default(repositoryOwnerDefault()).describe('GitHub Project owner.'),
		projectNumber: tool.schema.number().default(DEFAULT_PROJECT_NUMBER).describe('GitHub Project number.'),
	},
	async execute(args) {
		try {
			const project = defaults(args.owner, args.projectNumber);
			const target = requireTarget(args.itemId, args.issue, args.pr);
			const requested = normalize(args.sprint);
			const clear = requested === 'clear' || requested === 'none' || requested === 'unscheduled';
			return success(await setProjectFieldForTarget(project, target, 'Sprint', clear ? undefined : args.sprint, clear));
		} catch (error) {
			return failure('github_project_set_sprint', error);
		}
	},
});

export const set_estimate = tool({
	description: 'Mutates Project 5: set the Estimate number field, measured in hours, for a Mosaic item.',
	args: {
		issue: tool.schema.number().default(0).describe('Issue number to update. Use 0 when targeting PR or itemId.'),
		pr: tool.schema.number().default(0).describe(
			'Pull request number to update. Use 0 when targeting issue or itemId.',
		),
		itemId: tool.schema.string().default('').describe(
			'Project item id to update. Use empty string when targeting issue or PR.',
		),
		estimate: tool.schema.number().describe('Estimate value to set, measured in hours.'),
		owner: tool.schema.string().default(repositoryOwnerDefault()).describe('GitHub Project owner.'),
		projectNumber: tool.schema.number().default(DEFAULT_PROJECT_NUMBER).describe('GitHub Project number.'),
	},
	async execute(args) {
		try {
			const project = defaults(args.owner, args.projectNumber);
			const target = requireTarget(args.itemId, args.issue, args.pr);
			return success(await setProjectFieldForTarget(project, target, 'Estimate', args.estimate.toString(), false));
		} catch (error) {
			return failure('github_project_set_estimate', error);
		}
	},
});
