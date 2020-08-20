import os
import shutil
import requests
import concurrent.futures
from zipfile import ZipFile
from pathlib import Path

packages = {
    'data-plane-api': {'https://github.com/envoyproxy/data-plane-api/archive/master.zip': 'envoy'},
    'googleapis': {'https://github.com/googleapis/googleapis/archive/master.zip': 'google'},
    'udpa': {'https://github.com/cncf/udpa/archive/master.zip': 'udpa'},
    'protoc-gen-validate': {'https://github.com/envoyproxy/protoc-gen-validate/archive/master.zip': 'validate'}
}

protos = Path('proto')
if not protos.exists():
    protos.mkdir()
os.chdir(protos.absolute())


def download(repo_name, repo_url, repo_root):
    zipfile = f'{repo_name}.zip'
    if not Path(f'{repo_name}-master').exists():
        with open(zipfile, 'wb+') as f:
            for chunk in requests.get(repo_url, stream=True):
                f.write(chunk)

        with ZipFile(open(zipfile, 'rb')) as z:
            z.extractall()

    if not Path(repo_root).exists():
        shutil.copytree(Path(f'{repo_name}-master/{repo_root}'), repo_root)

    shutil.rmtree(f'{repo_name}-master')
    os.remove(zipfile)


def add_namespace(package):
    pkg = Path(f'envoy/api/v2/{package}')
    new_pkg = Path(f'envoy/api/v2/{package}NS')
    if new_pkg.exists() and not pkg.exists():
        return
    for file in pkg.iterdir():
        with open(file) as f:
            content = f.read().replace(
                f'package envoy.api.v2.{package};',
                f'package envoy.api.v2.{package}NS;',
            )
        with open(file, 'w+') as f:
            f.write(content)
    proto = Path(f'envoy/api/v2/{package}.proto')
    with open(proto) as f:
        content = f.read().replace(
            f'v2/{package}/',
            f'v2/{package}NS/',
        ).replace(
            f' {package}.',
            f' {package}NS.',
        )
    with open(proto, 'w+') as f:
        f.write(content)
    pkg.rename(new_pkg)


with concurrent.futures.ThreadPoolExecutor(max_workers=len(packages)) as executor:
    for name, opts in packages.items():
        url, dir_name = tuple(*opts.items())
        print(f'Downloading {dir_name}')
        executor.submit(
            download,
            name, url, dir_name
        )

add_namespace('cluster')
add_namespace('listener')
