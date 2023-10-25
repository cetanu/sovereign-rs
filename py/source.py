import os
import requests

URL = os.environ["OSB_URL"]


def main():
    response = requests.get(URL)
    response.raise_for_status()
    return response.text
