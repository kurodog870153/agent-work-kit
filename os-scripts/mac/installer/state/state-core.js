function validVersion(value) {
    return typeof value === 'string' && /^[0-9]+\.[0-9]+\.[0-9]+$/.test(value);
}

function run(argv) {
    if (argv[0] === 'release-version' && argv.length === 2) {
        var info = JSON.parse(argv[1]);
        if (!info || !validVersion(info.version)) throw new Error('invalid release version');
        return info.version;
    }
    if (argv[0] === 'tooling-release-version' && argv.length === 2) {
        var toolingInfo = JSON.parse(argv[1]);
        if (!toolingInfo || !validVersion(toolingInfo.tooling_version)) throw new Error('invalid Tooling release version');
        return toolingInfo.tooling_version;
    }
    if (argv[0] === 'work-release-version' && argv.length === 2) {
        var workInfo = JSON.parse(argv[1]);
        if (!workInfo || !validVersion(workInfo.work_version)) throw new Error('invalid Work release version');
        return workInfo.work_version;
    }
    if (argv[0] === 'combined' && argv.length === 5) {
        var combined;
        try { combined = JSON.parse(argv[1]); } catch (error) { return '-'; }
        var parent = argv[2].replace(/\/work$/, '');
        if (!combined || Object.keys(combined).sort().join(',') !== 'global_agent_dir,skill_install_dir,status,tooling_version,version,work_version' ||
                (combined.status !== 'installing' && combined.status !== 'installed') ||
                !validVersion(combined.version) || !validVersion(combined.work_version) ||
                !validVersion(combined.tooling_version) ||
                combined.skill_install_dir !== parent || argv[3] !== parent + '/tooling' ||
                (combined.global_agent_dir !== null &&
                    (typeof combined.global_agent_dir !== 'string' || combined.global_agent_dir.charAt(0) !== '/'))) return '-';
        if (argv[4] === 'tooling-version') return combined.tooling_version;
        if (argv[4] === 'work-version') return combined.work_version;
        if (argv[4] === 'version') return combined.version;
        if (argv[4] === 'global-agent-dir') return combined.global_agent_dir === null ? '-' : combined.global_agent_dir;
        if (argv[4] === 'status') return combined.status;
        return '-';
    }
    if (argv[0] === 'combined-create' && argv.length === 7) {
        if (!validVersion(argv[1]) || !validVersion(argv[2]) || !validVersion(argv[3]) ||
                argv[4].charAt(0) !== '/' ||
                (argv[5] !== '-' && argv[5].charAt(0) !== '/') ||
                (argv[6] !== 'installing' && argv[6] !== 'installed')) throw new Error('invalid combined state');
        return JSON.stringify({version: argv[1], work_version: argv[2], tooling_version: argv[3],
            skill_install_dir: argv[4], global_agent_dir: argv[5] === '-' ? null : argv[5], status: argv[6]});
    }
    throw new Error('invalid parent state operation');
}

if (typeof module !== 'undefined') module.exports = { run: run };
