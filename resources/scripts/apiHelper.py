#!/usr/bin/env python3
import argparse
import requests
import json

def start(url, location, filename):
    response = requests.post(f"{url}/print/start/{location}/{filename}")
    return body_or_status_code(response)

def cancel(url):
    response = requests.post(f"{url}/print/cancel")
    return body_or_status_code(response)

def pause(url):
    response = requests.post(f"{url}/print/pause")
    return body_or_status_code(response)

def resume(url):
    response = requests.post(f"{url}/print/resume")
    return body_or_status_code(response)

def status(url):
    response = requests.get(f"{url}/status")
    return body_or_status_code(response)

def manual_control(url, z, cure):
    response = requests.post(
        f"{url}/manual",
        params = {
            "z": z,
            "cure": cure,
        }
    )
    print(response.url)
    return body_or_status_code(response)

def list_files(url, location, page_index, page_size):
    response = requests.get(
        f"{url}/files",
        params = {
            "location": location,
            "page_index": page_index,
            "page_size": page_size
        }
    )
    return body_or_status_code(response)

def get_file(url, location, filename):
    response = requests.get(f"{url}/files/{location}/{filename}")
    return body_or_status_code(response)

def delete_file(url, location, filename):
    response = requests.delete(f"{url}/files/{location}/{filename}")
    return body_or_status_code(response)

def body_or_status_code(response):
    response.raise_for_status()
    try:
        return json.dumps(response.json(), indent=4)
    except:
        return response.status_code

def cli_status(args):
    return status(args.url)
def cli_resume(args):
    return resume(args.url)
def cli_pause(args):
    return pause(args.url)
def cli_cancel(args):
    return cancel(args.url)
def cli_start(args):
    return start(args.url, args.location, args.filename)
def cli_manual_control(args):
    return manual_control(args.url, args.z, args.cure)
def cli_list_files(args):
    return list_files(args.url, args.location, args.page_index, args.page_size)
def cli_get_file(args):
    return get_file(args.url, args.location, args.filename)
def cli_delete_file(args):
    return delete_file(args.url, args.location, args.filename)

def main():
    parser = argparse.ArgumentParser(
        description="This script provides an easier way to interact with the \
            Odyssey API from a local context, such as Klipper Macros or the \
            command line."
    )

    parser.add_argument('-u', '--url', default="http://127.0.0.1:12357")

    subparsers = parser.add_subparsers(
        required=True,
        title='API Endpoints',
        description='Valid CLI Endpoints',
        dest='endpoint',
        metavar=''
    )

    start_parser = subparsers.add_parser(
        'start',
        help='Start printing the specified file'
    )
    start_parser.add_argument('location')
    start_parser.add_argument('filename')
    start_parser.set_defaults(func=cli_start)
    
    cancel_parser = subparsers.add_parser(
        'cancel',
        help='cancel the current print (at the end of the current layer)'
    )
    cancel_parser.set_defaults(func=cli_cancel)

    pause_parser = subparsers.add_parser(
        'pause',
        help='Pause the current print (at the end of the current layer)'
    )
    pause_parser.set_defaults(func=cli_pause)

    resume_parser = subparsers.add_parser(
        'resume',
        help='Resume a previously paused print'
    )
    resume_parser.set_defaults(func=cli_resume)

    status_parser = subparsers.add_parser(
        'status',
        help='Return the current status from Odyssey'
    )
    status_parser.set_defaults(func=cli_status)

    manual_control_parser = subparsers.add_parser(
        'manual_control',
        help='Move the z axis of the printer, or toggle curing'
    )
    manual_control_parser.add_argument('-z', '--z', help="Desired position of the z axis")
    manual_control_parser.add_argument(
        '-c', '--cure',
        help="Desired curing status",
        choices=("true", "false")
    )
    manual_control_parser.set_defaults(func=cli_manual_control)

    args = parser.parse_args()

    print(args.func(args))

if __name__ == "__main__":
    main()