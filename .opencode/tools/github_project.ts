import { graphql } from '@octokit/graphql';
import { tool } from '@opencode-ai/plugin';

const DEFAULT_OWNER = 'kjanat';
const DEFAULT_PROJECT_NUMBER = 5;
const DEFAULT_REPOSITORY_OWNER = 'kjanat';
const DEFAULT_REPOSITORY_NAME = 'mosaic';
const ITEM_LIMIT = 200;
const GRAPHQL_TIMEOUT_MS = 30_000;

type JsonRecord = Record<string, unknown>;

type ProjectDefaults = {
	owner: string;
	projectNumber: number;
};

type ProjectItemSummary = {
	itemId: string;
	issueNumber: number | null;
	title: string;
	status: string;
	sprint: string;
	priority: string;
	area: string;
	url: string;
};

type ProjectField = {
	id: string;
	name: string;
	type: string;
	options: ReadonlyArray<ProjectOption>;
};

type ProjectOption = {
	id: string;
	name: string;
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

function defaults(owner: string | undefined, projectNumber: number | undefined): ProjectDefaults {
	return {
		owner: owner ?? DEFAULT_OWNER,
		projectNumber: projectNumber ?? DEFAULT_PROJECT_NUMBER,
	};
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

		return [{ id, name, type, options }];
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

function summarizeItem(item: JsonRecord): ProjectItemSummary {
	const content = recordValue(item, 'content');
	const issueNumber = content === null ? null : numberField(content, 'number');

	return {
		itemId: stringField(item, 'id') ?? '',
		issueNumber,
		title: (content === null ? null : stringField(content, 'title')) ?? stringField(item, 'title') ?? '',
		status: itemField(item, ['Status']),
		sprint: itemField(item, ['Sprint', 'Iteration']),
		priority: itemField(item, ['Priority']),
		area: itemField(item, ['Area']),
		url: (content === null ? null : stringField(content, 'url')) ?? '',
	};
}

function findItemByIssue(
	items: ReadonlyArray<JsonRecord>,
	issueNumber: number,
	projectNumber: number,
): ProjectItemSummary {
	const found = items.map(summarizeItem).find((item) => item.issueNumber === issueNumber);
	if (found === undefined) {
		throw new Error(`Issue #${issueNumber} is not in Project ${projectNumber}`);
	}
	if (found.itemId === '') {
		throw new Error(`Project item for #${issueNumber} has no item id`);
	}
	return found;
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
			return (left.issueNumber ?? Number.MAX_SAFE_INTEGER) - (right.issueNumber ?? Number.MAX_SAFE_INTEGER);
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

function markdownCell(value: string): string {
	return value.replace(/\|/g, '\\|');
}

function formatItems(items: ReadonlyArray<ProjectItemSummary>): string {
	if (items.length === 0) {
		return 'No matching project items.';
	}

	const rows = items.map((item) => {
		const number = item.issueNumber === null ? '' : `#${item.issueNumber}`;
		return [number, item.title, item.status, item.sprint, item.priority, item.area]
			.map(markdownCell)
			.join(' | ');
	});

	return [
		'| Issue | Title | Status | Sprint | Priority | Area |',
		'| --- | --- | --- | --- | --- | --- |',
		...rows.map((row) => `| ${row} |`),
	].join('\n');
}

function filterItems(
	items: ReadonlyArray<ProjectItemSummary>,
	status: string | undefined,
	sprint: string | undefined,
	includeDone: boolean | undefined,
): ReadonlyArray<ProjectItemSummary> {
	const statusFilter = status === undefined ? null : normalize(status);
	const sprintFilter = sprint === undefined ? null : normalize(sprint);
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

async function issueDetails(issueNumber: number): Promise<JsonRecord> {
	const query = `query($owner: String!, $name: String!, $number: Int!) {
    repository(owner: $owner, name: $name) {
      issue(number: $number) {
        number
        title
        state
        body
        url
        labels(first: 30) { nodes { name } }
      }
    }
  }`;
	const data = await githubGraphql(query, {
		owner: DEFAULT_REPOSITORY_OWNER,
		name: DEFAULT_REPOSITORY_NAME,
		number: issueNumber,
	});
	const repository = recordValue(data, 'repository');
	const issue = repository === null ? null : recordValue(repository, 'issue');
	if (issue === null) {
		throw new Error(`Issue #${issueNumber} not found in ${DEFAULT_REPOSITORY_OWNER}/${DEFAULT_REPOSITORY_NAME}`);
	}
	return issue;
}

async function issueNodeId(issueNumber: number): Promise<string> {
	const query = `query($owner: String!, $name: String!, $number: Int!) {
    repository(owner: $owner, name: $name) {
      issue(number: $number) { id }
    }
  }`;
	const data = await githubGraphql(query, {
		owner: DEFAULT_REPOSITORY_OWNER,
		name: DEFAULT_REPOSITORY_NAME,
		number: issueNumber,
	});
	const repository = recordValue(data, 'repository');
	const issue = repository === null ? null : recordValue(repository, 'issue');
	const id = issue === null ? null : stringField(issue, 'id');
	if (id === null) {
		throw new Error(`Issue #${issueNumber} not found in ${DEFAULT_REPOSITORY_OWNER}/${DEFAULT_REPOSITORY_NAME}`);
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

function formatFields(fields: ReadonlyArray<ProjectField>, iteration: IterationField | null): string {
	const rows = fields.map((field) => {
		const iterationOptions = iteration !== null && normalize(field.name) === normalize(iteration.name)
			? iteration.iterations.map((item) => item.title).join(', ')
			: null;
		const options = iterationOptions
			?? (field.options.length === 0 ? '' : field.options.map((option) => option.name).join(', '));
		return [field.name, field.type, options].map(markdownCell).join(' | ');
	});
	return ['| Field | Type | Options |', '| --- | --- | --- |', ...rows.map((row) => `| ${row} |`)].join('\n');
}

export const list = tool({
	description: 'List GitHub Project 5 items for Mosaic, with optional status/sprint filters.',
	args: {
		status: tool.schema.string().optional().describe('Optional status filter, e.g. Ready, Backlog, Done.'),
		sprint: tool.schema.string().optional().describe('Optional sprint title filter, or none/unscheduled.'),
		includeDone: tool.schema.boolean().optional().describe('Include Done items. Defaults to false.'),
		owner: tool.schema.string().optional().describe('GitHub owner. Defaults to kjanat.'),
		projectNumber: tool.schema.number().optional().describe('GitHub Project number. Defaults to 5.'),
	},
	async execute(args) {
		try {
			const project = defaults(args.owner, args.projectNumber);
			const items = (await rawItems(project)).map(summarizeItem);
			return formatItems(filterItems(items, args.status, args.sprint, args.includeDone));
		} catch (error) {
			return `github_project_list failed: ${errorMessage(error)}`;
		}
	},
});

export const queue = tool({
	description: 'Show the active Mosaic Project 5 queue, sorted by status and priority.',
	args: {
		includeBacklog: tool.schema.boolean().optional().describe('Include Backlog items. Defaults to false.'),
		owner: tool.schema.string().optional().describe('GitHub owner. Defaults to kjanat.'),
		projectNumber: tool.schema.number().optional().describe('GitHub Project number. Defaults to 5.'),
	},
	async execute(args) {
		try {
			const project = defaults(args.owner, args.projectNumber);
			const items = (await rawItems(project)).map(summarizeItem);
			return formatItems(queueItems(items, args.includeBacklog));
		} catch (error) {
			return `github_project_queue failed: ${errorMessage(error)}`;
		}
	},
});

export const fields = tool({
	description: 'List Mosaic GitHub Project 5 fields, options, and sprint choices.',
	args: {
		owner: tool.schema.string().optional().describe('GitHub owner. Defaults to kjanat.'),
		projectNumber: tool.schema.number().optional().describe('GitHub Project number. Defaults to 5.'),
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
			return `github_project_fields failed: ${errorMessage(error)}`;
		}
	},
});

export const view = tool({
	description: 'View one GitHub issue with its GitHub Project 5 fields.',
	args: {
		issue: tool.schema.number().describe('Issue number to inspect.'),
		owner: tool.schema.string().optional().describe('GitHub owner. Defaults to kjanat.'),
		projectNumber: tool.schema.number().optional().describe('GitHub Project number. Defaults to 5.'),
	},
	async execute(args) {
		try {
			const project = defaults(args.owner, args.projectNumber);
			const item = findItemByIssue(await rawItems(project), args.issue, project.projectNumber);
			const issue = await issueDetails(args.issue);
			const labelsConnection = recordValue(issue, 'labels');
			const labels = (labelsConnection === null ? [] : arrayField(labelsConnection, 'nodes')).filter(isRecord).flatMap(
				(label) => {
					const name = stringField(label, 'name');
					return name === null ? [] : [name];
				},
			);
			const body = stringField(issue, 'body') ?? '';
			return [
				`#${args.issue} ${stringField(issue, 'title') ?? item.title}`,
				`URL: ${stringField(issue, 'url') ?? item.url}`,
				`State: ${stringField(issue, 'state') ?? ''}`,
				`Status: ${item.status || '(none)'}`,
				`Sprint: ${item.sprint || '(none)'}`,
				`Priority: ${item.priority || '(none)'}`,
				`Area: ${item.area || '(none)'}`,
				`Labels: ${labels.join(', ') || '(none)'}`,
				'',
				body,
			].join('\n');
		} catch (error) {
			return `github_project_view failed: ${errorMessage(error)}`;
		}
	},
});

export const add_issue = tool({
	description: 'Add a Mosaic GitHub issue to Project 5, optionally setting status and sprint.',
	args: {
		issue: tool.schema.number().describe('Issue number to add to Project 5.'),
		status: tool.schema.string().optional().describe('Optional status to set after adding, e.g. Ready.'),
		sprint: tool.schema.string().optional().describe('Optional sprint title/id, current, or next.'),
		owner: tool.schema.string().optional().describe('GitHub owner. Defaults to kjanat.'),
		projectNumber: tool.schema.number().optional().describe('GitHub Project number. Defaults to 5.'),
	},
	async execute(args) {
		try {
			const project = defaults(args.owner, args.projectNumber);
			const id = await projectId(project);
			const itemId = await addIssueItem(id, await issueNodeId(args.issue));
			const updates: Array<string> = [`added #${args.issue}`];

			if (args.status !== undefined) {
				const statusField = findField(await rawFields(project), 'Status');
				const option = findOption(statusField, args.status);
				await setSingleSelectField(id, itemId, statusField.id, option.id);
				updates.push(`status ${option.name}`);
			}

			if (args.sprint !== undefined) {
				const field = await iterationField(project);
				const iteration = resolveIteration(field.iterations, args.sprint);
				if (iteration === null) {
					const choices = field.iterations.map((candidate) => candidate.title).join(', ');
					throw new Error(`Sprint ${args.sprint} not found. Choices: current, next, ${choices}`);
				}
				await setIterationField(id, itemId, field.id, iteration.id);
				updates.push(`${field.name} ${iteration.title}`);
			}

			return `Project 5 item updated: ${updates.join(', ')}.`;
		} catch (error) {
			return `github_project_add_issue failed: ${errorMessage(error)}`;
		}
	},
});

export const set_status = tool({
	description: 'Set the Status field for a Mosaic GitHub Project 5 issue.',
	args: {
		issue: tool.schema.number().describe('Issue number to update.'),
		status: tool.schema.string().describe('Status name, e.g. Backlog, Ready, In progress, In review, Done.'),
		owner: tool.schema.string().optional().describe('GitHub owner. Defaults to kjanat.'),
		projectNumber: tool.schema.number().optional().describe('GitHub Project number. Defaults to 5.'),
	},
	async execute(args) {
		try {
			const project = defaults(args.owner, args.projectNumber);
			const [id, item, fields] = await Promise.all([
				projectId(project),
				rawItems(project).then((items) => findItemByIssue(items, args.issue, project.projectNumber)),
				rawFields(project),
			]);
			const statusField = findField(fields, 'Status');
			const option = findOption(statusField, args.status);
			await setSingleSelectField(id, item.itemId, statusField.id, option.id);
			return `Set #${args.issue} status to ${option.name}.`;
		} catch (error) {
			return `github_project_set_status failed: ${errorMessage(error)}`;
		}
	},
});

export const set_select = tool({
	description: 'Set any single-select field for a Mosaic GitHub Project 5 issue.',
	args: {
		issue: tool.schema.number().describe('Issue number to update.'),
		field: tool.schema.string().describe('Single-select field name, e.g. Priority, Area, Phase, Type, Size.'),
		option: tool.schema.string().describe('Option name to set.'),
		owner: tool.schema.string().optional().describe('GitHub owner. Defaults to kjanat.'),
		projectNumber: tool.schema.number().optional().describe('GitHub Project number. Defaults to 5.'),
	},
	async execute(args) {
		try {
			const project = defaults(args.owner, args.projectNumber);
			const [id, item, projectFields] = await Promise.all([
				projectId(project),
				rawItems(project).then((items) => findItemByIssue(items, args.issue, project.projectNumber)),
				rawFields(project),
			]);
			const field = findField(projectFields, args.field);
			const option = findOption(field, args.option);
			await setSingleSelectField(id, item.itemId, field.id, option.id);
			return `Set #${args.issue} ${field.name} to ${option.name}.`;
		} catch (error) {
			return `github_project_set_select failed: ${errorMessage(error)}`;
		}
	},
});

export const set_sprint = tool({
	description: 'Set or clear the Sprint/Iteration field for a Mosaic GitHub Project 5 issue.',
	args: {
		issue: tool.schema.number().describe('Issue number to update.'),
		sprint: tool.schema
			.string()
			.describe('Sprint title/id, current, next, or clear/none/unscheduled to remove sprint.'),
		owner: tool.schema.string().optional().describe('GitHub owner. Defaults to kjanat.'),
		projectNumber: tool.schema.number().optional().describe('GitHub Project number. Defaults to 5.'),
	},
	async execute(args) {
		try {
			const project = defaults(args.owner, args.projectNumber);
			const [id, item, field] = await Promise.all([
				projectId(project),
				rawItems(project).then((items) => findItemByIssue(items, args.issue, project.projectNumber)),
				iterationField(project),
			]);

			const requested = normalize(args.sprint);
			if (requested === 'clear' || requested === 'none' || requested === 'unscheduled') {
				await clearProjectField(id, item.itemId, field.id);
				return `Cleared sprint for #${args.issue}.`;
			}

			const iteration = resolveIteration(field.iterations, args.sprint);
			if (iteration === null) {
				const choices = field.iterations.map((candidate) => candidate.title).join(', ');
				throw new Error(`Sprint ${args.sprint} not found. Choices: current, next, clear, ${choices}`);
			}

			await setIterationField(id, item.itemId, field.id, iteration.id);
			return `Set #${args.issue} ${field.name} to ${iteration.title}.`;
		} catch (error) {
			return `github_project_set_sprint failed: ${errorMessage(error)}`;
		}
	},
});

export const set_estimate = tool({
	description: 'Set the Estimate number field for a Mosaic GitHub Project 5 issue.',
	args: {
		issue: tool.schema.number().describe('Issue number to update.'),
		estimate: tool.schema.number().describe('Estimate value to set.'),
		owner: tool.schema.string().optional().describe('GitHub owner. Defaults to kjanat.'),
		projectNumber: tool.schema.number().optional().describe('GitHub Project number. Defaults to 5.'),
	},
	async execute(args) {
		try {
			const project = defaults(args.owner, args.projectNumber);
			const [id, item, projectFields] = await Promise.all([
				projectId(project),
				rawItems(project).then((items) => findItemByIssue(items, args.issue, project.projectNumber)),
				rawFields(project),
			]);
			const field = findField(projectFields, 'Estimate');
			await setNumberField(id, item.itemId, field.id, args.estimate);
			return `Set #${args.issue} ${field.name} to ${args.estimate}.`;
		} catch (error) {
			return `github_project_set_estimate failed: ${errorMessage(error)}`;
		}
	},
});
