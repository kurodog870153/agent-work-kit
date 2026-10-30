const assert = require('assert');
const fs = require('fs');
const state = require('../../../../../../os-scripts/mac/installer/state/work-state.js');
const core = require('../../../../../../os-scripts/mac/installer/state/state-core.js');
const template = fs.readFileSync('os-scripts/common/installer/install-state.json', 'utf8');
assert.deepStrictEqual(Object.keys(JSON.parse(template)).sort(),
    ['global_agent_dir', 'skill_install_dir', 'status', 'tooling_version', 'version', 'work_version']);
const workspaceVersion = fs.readFileSync('VERSION', 'utf8').trim();
const toolingVersion = fs.readFileSync('rust/Cargo.toml', 'utf8').match(/^version = "([^"]+)"$/m)[1];
const aggregate = core.run(['combined-create', workspaceVersion, workspaceVersion, toolingVersion,
    '/tmp/skills', '-', 'installed']);
assert.strictEqual(core.run(['combined', aggregate, '/tmp/skills/work', '/tmp/skills/tooling', 'status']), 'installed');
assert.strictEqual(core.run(['combined', aggregate, '/tmp/skills/work', '/tmp/skills/tooling', 'tooling-version']), toolingVersion);
assert.strictEqual(core.run(['combined', aggregate, '/tmp/other/work', '/tmp/other/tooling', 'status']), '-');
const fromCombined = state.run(['from-combined',
    core.run(['combined', aggregate, '/tmp/skills/work', '/tmp/skills/tooling', 'work-version']),
    '/tmp/skills/work', core.run(['combined', aggregate, '/tmp/skills/work', '/tmp/skills/tooling', 'global-agent-dir'])]);
assert.deepStrictEqual(JSON.parse(fromCombined), {
    version: workspaceVersion, skills: state.expandSkills('all'),
    work_dir: '/tmp/skills/work', global_agents_dir: null
});

assert.deepStrictEqual(state.expandSkills('java,spring-boot'), ['general', 'programming-language', 'java', 'spring-boot']);
assert.strictEqual(state.expandSkills('all').length, 14);
const created = state.createState(template, '1.2.3', 'java,spring-boot', true,
    '/tmp/目錄 有空白/skills/work', '/tmp/全域 有空白');
const raw = JSON.stringify(created);
assert.deepStrictEqual(state.parseState(raw, created.work_dir), created);
assert.deepStrictEqual(JSON.parse(state.run(['validate', raw, created.work_dir])).state, created);
assert.strictEqual(state.parseState(raw, '/tmp/other'), null);
assert.strictEqual(state.parseState('', created.work_dir), null);
assert.strictEqual(state.parseState('{', created.work_dir), null);
for (const change of [
    { version: 'invalid' }, { skills: ['general', 'spring-boot'] },
    { skills: ['general', 'general'] }, { skills: ['unknown'] },
    { work_dir: '../relative' }, { global_agents_dir: 'relative' },
    { global_agents_dir: false }
]) {
    assert.strictEqual(state.parseState(JSON.stringify({ ...created, ...change }), created.work_dir), null);
}
assert.strictEqual(state.parseState(JSON.stringify({ ...created, extra: true }), created.work_dir), null);
const missing = { ...created };
delete missing.version;
assert.strictEqual(state.parseState(JSON.stringify(missing), created.work_dir), null);
assert.strictEqual(state.parseState(JSON.stringify({
    schema_version: 1, version: '1.2.3', skills: created.skills,
    global_agents: true, install_dir: created.work_dir
}), created.work_dir), null);
assert.throws(() => state.run(['create', template, '1.2.3', 'general', 'maybe', '/tmp/work', '/tmp/global']));
assert.throws(() => state.createState(template, '1.2.3', 'bad', true, '/tmp/work', '/tmp/global'));
assert.throws(() => state.createState(template, '1.2.3', 'general', true, 'relative', '/tmp/global'));
assert.throws(() => state.createState('{"version":null}', '1.2.3', 'general', true, '/tmp/work', '/tmp/global'));
const disabled = state.createState(template, '1.2.3', 'general', false, '/tmp/work', '/tmp/global');
assert.strictEqual(disabled.global_agents_dir, null);
assert.strictEqual(state.resolveSettings(JSON.stringify(disabled), '/tmp/work', null, null).global_agents, false);
assert.deepStrictEqual(state.resolveSettings('', created.work_dir, null, null),
    { has_valid_state: false, skills: ['general'], global_agents: true, global_agents_dir: null });
assert.deepStrictEqual(state.resolveSettings(raw, created.work_dir, null, null),
    { has_valid_state: true, skills: created.skills, global_agents: true,
        global_agents_dir: created.global_agents_dir });
assert.deepStrictEqual(state.resolveSettings(raw, created.work_dir, 'web', false),
    { has_valid_state: true, skills: ['general', 'web'], global_agents: false,
        global_agents_dir: created.global_agents_dir });
assert.strictEqual(state.run(['settings', '', created.work_dir, '', '']), '0\tgeneral\t1\t-\t-');
assert.strictEqual(state.run(['settings', raw, created.work_dir, 'web', 'false']),
    '1\tgeneral,web\t0\t1.2.3\t/tmp/全域 有空白');
assert.throws(() => state.run(['settings', raw, created.work_dir, '', 'invalid']));
assert.strictEqual(core.run(['release-version', '{"version":"1.2.3"}']), '1.2.3');
assert.throws(() => core.run(['release-version', '{"version":"bad"}']));
console.log('macOS install-state cases passed.');
