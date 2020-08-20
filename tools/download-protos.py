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

folder = Path('proto')
if not folder.exists():
    folder.mkdir()
os.chdir(folder.absolute())

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
