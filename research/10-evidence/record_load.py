#!/usr/bin/env python3
"""Run one measurement command, preserving host load before, during and after it."""
import argparse
import datetime
import json
import os
import platform
from pathlib import Path
import subprocess
import time


def snapshot(phase, command):
    result = subprocess.run(['ps', '-axo', 'pid=,pcpu=,comm='], capture_output=True, text=True, check=True)
    processes = []
    for line in result.stdout.splitlines():
        fields = line.strip().split(None, 2)
        if len(fields) == 3:
            processes.append({'pid': int(fields[0]), 'cpu_percent': float(fields[1]),
                              'executable': Path(fields[2]).name})
    if platform.system() == 'Darwin':
        cpu_model = subprocess.run(['sysctl', '-n', 'machdep.cpu.brand_string'], capture_output=True, text=True, check=True).stdout.strip()
        memory = subprocess.run(['vm_stat'], capture_output=True, text=True, check=True).stdout
        kernel_load = None
    else:
        cpu_model = next((line.split(':', 1)[1].strip() for line in Path('/proc/cpuinfo').read_text().splitlines() if line.startswith('model name')), platform.machine())
        memory = '\n'.join(Path('/proc/meminfo').read_text().splitlines()[:5])
        kernel_load = Path('/proc/loadavg').read_text().strip()
    return {'cpu_model': cpu_model, 'memory_summary': memory, 'proc_loadavg': kernel_load,
            'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
            'phase': phase, 'command': command, 'logical_cpus': os.cpu_count(),
            'load_average_1_5_15': os.getloadavg(),
            'uptime': subprocess.run(['uptime'], capture_output=True, text=True, check=True).stdout.strip(),
            'sum_process_cpu_percent': sum(p['cpu_percent'] for p in processes),
            'top_cpu': sorted(processes, key=lambda p: p['cpu_percent'], reverse=True)[:12]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--log', type=Path, required=True)
    parser.add_argument('--interval', type=float, default=15)
    parser.add_argument('command', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ['--'] else args.command
    if not command or args.interval <= 0:
        parser.error('provide a command and positive interval')
    args.log.parent.mkdir(parents=True, exist_ok=True)
    with args.log.open('x') as log:
        def record(phase, **extra):
            log.write(json.dumps(snapshot(phase, command) | extra) + '\n')
            log.flush()
        record('before')
        start = time.monotonic()
        process = subprocess.Popen(command)
        while True:
            try:
                code = process.wait(timeout=args.interval)
                break
            except subprocess.TimeoutExpired:
                record('during')
        record('after', exit_code=code, elapsed_seconds=time.monotonic() - start)
    return code


if __name__ == '__main__':
    raise SystemExit(main())
