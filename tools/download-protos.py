import os
import requests
from zipfile import ZipFile
from pathlib import Path
import shutil

packages = {
    'data-plane-api': {'https://github.com/envoyproxy/data-plane-api/archive/master.zip': 'envoy'},
    'googleapis': {'https://github.com/googleapis/googleapis/archive/master.zip': 'google'},
    'udpa': {'https://github.com/cncf/udpa/archive/master.zip': 'udpa'},
    'protoc-gen-validate': {'https://github.com/envoyproxy/protoc-gen-validate/archive/master.zip': 'validate'}
}

protos = Path('proto')

def download():
    if not protos.exists():
        protos.mkdir()
    os.chdir(protos.absolute())

    for name, opts in packages.items():
        url, dir_name = tuple(*opts.items())

        if not Path(f'{name}-master').exists():
            print(f'Downloading {name}', end='... ')
            with open('dl.zip', 'wb+') as f:
                for chunk in requests.get(url, stream=True):
                    f.write(chunk)
            print('Done')

            print('Extracting', end='... ')
            with ZipFile(open('dl.zip', 'rb')) as z:
                z.extractall()
            print('Done')

        if not Path(dir_name).exists():
            shutil.copytree(Path(f'{name}-master/{dir_name}'), dir_name)

        shutil.rmtree(f'{name}-master')
        os.remove('dl.zip')


def add_namespace(package):
    pkg = protos.joinpath(Path(f'envoy/api/v2/{package}'))
    new_pkg = protos.joinpath(Path(f'envoy/api/v2/{package}NS'))
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
    pkg.rename(new_pkg)


download()
add_namespace('cluster')
add_namespace('listener')
