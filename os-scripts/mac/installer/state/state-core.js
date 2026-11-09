function validVersion(value) {
    return typeof value === 'string' && /^[0-9]+\.[0-9]+\.[0-9]+$/.test(value);
}

function run(argv) {
    if ((argv[0] === 'release-host' || argv[0] === 'release-target') && argv.length === 2) {
        var identity = JSON.parse(argv[1]);
        if (!identity || (identity.host !== 'codex' && identity.host !== 'claude') ||
                (identity.target !== 'aarch64-apple-darwin' && identity.target !== 'x86_64-pc-windows-msvc')) {
            throw new Error('invalid release identity');
        }
        return argv[0] === 'release-host' ? identity.host : identity.target;
    }
    if (argv[0] === 'release-version' && argv.length === 2) {
        var info = JSON.parse(argv[1]);
        if (!info || !validVersion(info.version)) throw new Error('invalid release version');
        return info.version;
    }
    if (argv[0] === 'release-rust-workspace-version' && argv.length === 2) {
        var releaseInfo = JSON.parse(argv[1]);
        if (!releaseInfo || !validVersion(releaseInfo.rust_workspace_version)) throw new Error('invalid Rust workspace release version');
        return releaseInfo.rust_workspace_version;
    }
    if (argv[0] === 'combined' && argv.length === 6) {
        var combined;
        try { combined = JSON.parse(argv[1]); } catch (error) { return '-'; }
        var parent = argv[2].replace(/\/work$/, '');
        var selectedHost = argv[5];
        if (!combined || Object.keys(combined).sort().join(',') !== 'global_instructions_dir,host,rust_workspace_version,skill_install_dir,status,version' ||
                (selectedHost !== 'codex' && selectedHost !== 'claude') || combined.host !== selectedHost ||
                (combined.status !== 'installing' && combined.status !== 'installed') ||
                !validVersion(combined.version) || !validVersion(combined.rust_workspace_version) ||
                combined.skill_install_dir !== parent || argv[3] !== parent + '/tooling' ||
                (combined.global_instructions_dir !== null &&
                    (typeof combined.global_instructions_dir !== 'string' || combined.global_instructions_dir.charAt(0) !== '/'))) return '-';
        if (argv[4] === 'rust-workspace-version') return combined.rust_workspace_version;
        if (argv[4] === 'version') return combined.version;
        if (argv[4] === 'global-instructions-dir') return combined.global_instructions_dir === null ? '-' : combined.global_instructions_dir;
        if (argv[4] === 'status') return combined.status;
        return '-';
    }
    if (argv[0] === 'combined-create' && argv.length === 7) {
        var host = argv[6];
        if (host !== 'codex' && host !== 'claude') throw new Error('invalid installer host');
        if (!validVersion(argv[1]) || !validVersion(argv[2]) ||
                argv[3].charAt(0) !== '/' ||
                (argv[4] !== '-' && argv[4].charAt(0) !== '/') ||
                (argv[5] !== 'installing' && argv[5] !== 'installed')) throw new Error('invalid combined state');
        return JSON.stringify({host: host, version: argv[1], rust_workspace_version: argv[2],
            skill_install_dir: argv[3], global_instructions_dir: argv[4] === '-' ? null : argv[4], status: argv[5]});
    }
    throw new Error('invalid parent state operation');
}

if (typeof module !== 'undefined') module.exports = { run: run };
