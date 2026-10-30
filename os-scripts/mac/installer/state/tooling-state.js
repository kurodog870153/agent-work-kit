function validVersion(value) {
    return typeof value === 'string' && /^[0-9]+\.[0-9]+\.[0-9]+$/.test(value);
}

function parseState(raw, target) {
    var value;
    try { value = JSON.parse(raw); } catch (_) { return null; }
    if (!value || typeof value !== 'object' || Array.isArray(value) ||
            Object.keys(value).sort().join(',') !== 'skill,tooling_dir,version' ||
            value.skill !== 'tooling' || value.tooling_dir !== target ||
            !validVersion(value.version)) return null;
    return value;
}

function run(argv) {
    if (argv[0] === 'create' && argv.length === 3) {
        if (!validVersion(argv[1]) || typeof argv[2] !== 'string' ||
                argv[2].charAt(0) !== '/' || !argv[2].endsWith('/tooling')) {
            throw new Error('invalid Tooling state input');
        }
        return JSON.stringify({ version: argv[1], skill: 'tooling', tooling_dir: argv[2] });
    }
    if (argv[0] === 'version' && argv.length === 3) {
        var state = parseState(argv[1], argv[2]);
        return state ? state.version : '-';
    }
    throw new Error('invalid Tooling state operation');
}

if (typeof module !== 'undefined') module.exports = { parseState: parseState, run: run };
