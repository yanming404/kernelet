#!/usr/bin/env python3
"""Run platform and TypedIO lifecycle regressions in complete Asterinas under KVM.

Build with tests/typedio-kvm/build.sh first. Run serially: this runner shares
the prototype staging paths. Outputs remain in .build/typedio-kvm/LABEL.
"""
# SPDX-License-Identifier: MPL-2.0
import argparse
import datetime
import hashlib
import json
import os
import shutil
import re
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument('--label', required=True)
parser.add_argument('--vcpus', type=int, choices=(1, 2, 4, 8), default=1)
parser.add_argument('--expect-marker')
parser.add_argument('--cmdline', default='console=hvc0 loglevel=warn -- platform')
parser.add_argument('--cpu-model', default='host')
parser.add_argument('--migrate', action='store_true')
parser.add_argument('--vsock', action='store_true')
parser.add_argument('--guest-pair', action='store_true')
parser.add_argument('--lifecycle', choices=('backlog', 'reset', 'gh-reset'))
args = parser.parse_args()
if args.expect_marker is None:
    args.expect_marker = ({'backlog': 'LIFECYCLE BACKLOG PASS', 'reset': 'LIFECYCLE GUEST PASS', 'gh-reset': 'LIFECYCLE HOST PASS'}[args.lifecycle] if args.lifecycle
        else 'SMOKE TENANT PASS all' if args.vsock or args.guest_pair else 'PLATFORM PASS all')
if not args.label.replace('-', '').isalnum():
    parser.error('a simple unique label is required')
root = Path(__file__).resolve().parents[2]
out = root / '.build/typedio-kvm' / args.label
out.mkdir(parents=True, exist_ok=False)
metadata = dict(label=args.label, vcpus=args.vcpus, attachments=[],
    expected_marker=args.expect_marker, kernel_cmdline=args.cmdline,
    timestamp=datetime.datetime.now(datetime.timezone(datetime.timedelta(hours=8))).isoformat(),
    host_kernel=os.uname().release,
    kernelet_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
    asterinas_commit=subprocess.check_output(['git', '-C', str(root.parents[1] / 'asterinas'), 'rev-parse', 'HEAD'], text=True).strip())
if args.vsock:
    metadata['attachments'] = [{'cid': 2, 'peers': [42], 'host_listener': 8000}, {'cid': 42, 'peers': [2], 'before_carrier_start': True}]
if args.guest_pair:
    metadata['attachments'] = [{'cid': 7, 'peers': [42], 'instance': 'server'}, {'cid': 42, 'peers': [7], 'instance': 'client'}]
metadata['lifecycle_scenario'] = args.lifecycle
if args.lifecycle:
    metadata['attachments'] = ([{'cid': 2, 'peers': [42], 'host_listener': 6000}, {'cid': 42, 'peers': [2], 'same_cid_replacement': True}] if args.lifecycle == 'gh-reset'
        else [{'cid': 7, 'peers': [42], 'instance': 'server'}, {'cid': 42, 'peers': [7] if args.lifecycle == 'backlog' else [2, 7], 'instance': 'client'}])
metadata['harness_sha256'] = {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
    for p in sorted((root / 'tests/typedio-kvm').iterdir()) if p.is_file() and p.suffix in ('.c', '.h', '.S', '.py')}
stage = ['make', '-f', 'tests/typedio-kvm/Makefile', 'KLET_KERNEL=asterinas',
         f'TYPEDIO_VCPUS={args.vcpus}', f'TYPEDIO_CMDLINE={args.cmdline}',
         f'TYPEDIO_LIFECYCLE={args.lifecycle or "reset"}',
         'stage-typedio-host-lifecycle' if args.lifecycle == 'gh-reset' else 'stage-typedio-lifecycle' if args.lifecycle else 'stage-typedio-guest-pair' if args.guest_pair else 'stage-typedio-vsock' if args.vsock else 'stage-typedio-migration' if args.migrate else 'stage-typedio-bootstrap', 'initramfs']
metadata['stage_command'] = stage
with (out / 'build.log').open('w') as log:
    result = subprocess.run(stage, cwd=root, stdout=log, stderr=subprocess.STDOUT)
metadata['stage_exit_code'] = result.returncode
if result.returncode:
    (out / 'environment.json').write_text(json.dumps(metadata, indent=2) + '\n')
    raise SystemExit(result.returncode)
paths = ['.build/linux-6.12/arch/x86/boot/bzImage', '.build/linux-6.12/.config',
         'linux/endovisor/endovisor.ko', '.build/asterinas.image', '.build/initramfs.cpio.gz']
metadata['sha256'] = {p: hashlib.sha256((root / p).read_bytes()).hexdigest() for p in paths}
shutil.copy2(root / '.build/asterinas.image', out / 'asterinas.image')
for name, repo in (('kernelet', root.parent), ('asterinas', root.parents[1] / 'asterinas')):
    patch = subprocess.check_output(['git', '-C', str(repo), 'diff', 'HEAD'])
    (out / f'{name}.patch').write_bytes(patch)
    metadata[f'{name}_patch_sha256'] = hashlib.sha256(patch).hexdigest()
command = ['qemu-system-x86_64', '-enable-kvm', '-cpu', args.cpu_model, '-smp', str(max(2, args.vcpus)),
           '-m', '512M', '-nographic', '-no-reboot', '-monitor', 'none',
           '-kernel', str(root / paths[0]), '-initrd', str(root / paths[4]),
           '-append', 'console=ttyS0 panic=-1', '-serial', f'file:{out / "serial.log"}']
metadata['qemu_command'] = command
with (out / 'qemu.log').open('w') as log:
    try:
        result = subprocess.run(command, cwd=root, stdin=subprocess.DEVNULL,
                                stdout=log, stderr=subprocess.STDOUT, timeout=120)
        metadata['qemu_exit_code'] = result.returncode
    except subprocess.TimeoutExpired:
        metadata['qemu_exit_code'] = 'TIMEOUT'
serial = (out / 'serial.log').read_text(errors='replace') if (out / 'serial.log').exists() else ''
checks = {
    'qemu_clean_exit': metadata['qemu_exit_code'] == 0,
    'guest_linux_612': 'Linux version 6.12.0' in serial,
    'endovisor_abi8': 'image ABI 8' in serial,
    'all_vcpus_started': all(f'vOSTD: virtual CPU {i} up' in serial for i in range(1, args.vcpus)),
    'carrier_migration': not args.migrate or 'MIGRATION PASS real carrier observed' in serial,
    'host_adapter': not args.vsock or 'SMOKE HOST PASS all' in serial,
    'guest_pair': not args.guest_pair or ('SMOKE GUEST-GUEST PASS two complete instances' in serial and 'SMOKE SERVER PASS all' in serial),
    'lifecycle': not args.lifecycle or {'backlog': 'LIFECYCLE BACKLOG PASS', 'reset': 'LIFECYCLE GUEST PASS', 'gh-reset': 'LIFECYCLE HOST PASS'}[args.lifecycle] in serial,
    'complete_userspace_init': args.expect_marker in serial,
    'endovisor_unloaded': 'kernelet: endovisor unloaded' in serial,
    'guest_finished': 'INIT: done' in serial,
    'zero_live_resources': all('0 live' in line for line in serial.splitlines() if any(token in line for token in ('kernelet: heap:', 'kernelet: kernelet stacks:', 'kernelet: carrier records:'))),
    'no_kernel_error': not any(s in serial for s in ('WARNING:', 'BUG:', 'Oops:', 'Kernel panic', 'panicked', 'PANIC', 'EXPERIMENT FAIL', 'delete_module endovisor failed')),
}
if not (args.vsock or args.guest_pair or args.lifecycle):
    calls = [int(value) for value in re.findall(r'(\d+) service call\(s\)', serial)]
    checks['bounded_idle_services'] = bool(calls) and max(calls) < 1000000 * args.vcpus
metadata['checks'] = checks
metadata['status'] = 'PASS' if all(checks.values()) else 'FAIL'
(out / 'environment.json').write_text(json.dumps(metadata, indent=2) + '\n')
print(json.dumps(dict(status=metadata['status'], checks=checks, logs=str(out)), indent=2))
raise SystemExit(0 if metadata['status'] == 'PASS' else 1)
