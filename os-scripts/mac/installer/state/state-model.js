// Pure state logic shared by osascript (run) and the local Node test harness.
var skillOrder = ['general', 'web', 'backend', 'frontend', 'astro', 'css', 'tailwind',
    'programming-language', 'java', 'spring-boot', 'persistence', 'jpa', 'mybatis', 'typescript'];
var skillParents = {
    general: [], web: ['general'], backend: ['web'], frontend: ['web'],
    astro: ['frontend'], css: ['frontend'], tailwind: ['css'],
    'programming-language': ['general'], java: ['programming-language'],
    'spring-boot': ['java'], persistence: ['java'], jpa: ['persistence'],
    mybatis: ['persistence'], typescript: ['programming-language']
};

function expandSkills(value) {
    var selected = value === 'all' ? skillOrder.slice() : value.split(',');
    var seenInput = {};
    var included = { general: true };
    function add(id) {
        var parents = skillParents[id];
        for (var i = 0; i < parents.length; i++) add(parents[i]);
        included[id] = true;
    }
    for (var i = 0; i < selected.length; i++) {
        var id = selected[i];
        if (!Object.prototype.hasOwnProperty.call(skillParents, id) || seenInput[id]) throw new Error('invalid Skill ID');
        seenInput[id] = true;
        add(id);
    }
    return skillOrder.filter(function (id) { return included[id] === true; });
}

function validVersion(value) {
    return typeof value === 'string' && /^[0-9]+\.[0-9]+\.[0-9]+$/.test(value);
}

function createState(templateRaw, version, skills, globalAgents, workDir, globalAgentsDir) {
    var template = JSON.parse(templateRaw);
    if (!template || typeof template !== 'object' || Array.isArray(template) ||
            Object.keys(template).sort().join(',') !== 'global_agents_dir,skills,version,work_dir' ||
            Object.keys(template).some(function (key) { return template[key] !== null; }) ||
            !validVersion(version) || typeof workDir !== 'string' || workDir.charAt(0) !== '/' ||
            typeof globalAgents !== 'boolean' ||
            (globalAgents && (typeof globalAgentsDir !== 'string' || globalAgentsDir.charAt(0) !== '/'))) {
        throw new Error('invalid state input or template');
    }
    return {
        version: version,
        skills: expandSkills(skills),
        work_dir: workDir,
        global_agents_dir: globalAgents ? globalAgentsDir : null
    };
}

function parseState(raw, workDir) {
    var value;
    try { value = JSON.parse(raw); } catch (_) { return null; }
    if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
    var keys = Object.keys(value).sort().join(',');
    if (keys !== 'global_agents_dir,skills,version,work_dir' ||
            !validVersion(value.version) || value.work_dir !== workDir ||
            (value.global_agents_dir !== null &&
                (typeof value.global_agents_dir !== 'string' ||
                value.global_agents_dir.charAt(0) !== '/')) ||
            !Array.isArray(value.skills) || value.skills.length === 0 ||
            value.skills.some(function (id) { return typeof id !== 'string'; })) return null;
    try {
        if (expandSkills(value.skills.join(',')).join(',') !== value.skills.join(',')) return null;
    } catch (_) { return null; }
    return value;
}

function resolveSettings(raw, workDir, skillsOverride, globalAgentsOverride) {
    var state = parseState(raw, workDir);
    if (skillsOverride !== null && typeof skillsOverride !== 'string') throw new Error('invalid Skills override');
    if (globalAgentsOverride !== null && typeof globalAgentsOverride !== 'boolean') throw new Error('invalid Global AGENTS override');
    return {
        has_valid_state: state !== null,
        skills: skillsOverride !== null ? expandSkills(skillsOverride) : (state ? state.skills : ['general']),
        global_agents: globalAgentsOverride !== null ? globalAgentsOverride : (state ? state.global_agents_dir !== null : true),
        global_agents_dir: state ? state.global_agents_dir : null
    };
}

function run(argv) {
    if (argv[0] === 'create' && argv.length === 7) {
        if (argv[4] !== 'true' && argv[4] !== 'false') throw new Error('invalid Global AGENTS value');
        return JSON.stringify(createState(argv[1], argv[2], argv[3], argv[4] === 'true', argv[5], argv[6]));
    }
    if (argv[0] === 'validate' && argv.length === 3) {
        var state = parseState(argv[1], argv[2]);
        return JSON.stringify({ valid: state !== null, state: state });
    }
    if (argv[0] === 'settings' && argv.length === 5) {
        var skillsOverride = argv[3] === '' ? null : argv[3];
        var globalOverride = argv[4] === '' ? null : argv[4] === 'true' ? true : argv[4] === 'false' ? false : undefined;
        if (globalOverride === undefined) throw new Error('invalid Global AGENTS override');
        var settings = resolveSettings(argv[1], argv[2], skillsOverride, globalOverride);
        var prior = parseState(argv[1], argv[2]);
        return [settings.has_valid_state ? '1' : '0', settings.skills.join(','),
            settings.global_agents ? '1' : '0', prior ? prior.version : '-',
            settings.global_agents_dir || '-'].join('\t');
    }
    if (argv[0] === 'release-version' && argv.length === 2) {
        var info = JSON.parse(argv[1]);
        if (!info || !validVersion(info.version)) throw new Error('invalid release version');
        return info.version;
    }
    throw new Error('invalid install-state operation');
}

if (typeof module !== 'undefined') module.exports = { expandSkills: expandSkills, createState: createState, parseState: parseState, resolveSettings: resolveSettings, run: run };
