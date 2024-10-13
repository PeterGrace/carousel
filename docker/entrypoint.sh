#!/bin/bash

if [[ ! -z "$DEBUG" ]];
then
    echo "DEBUG requested, sleep infinity"
    /bin/sleep infinity
    exit 1
fi
cd /opt/carousel
./carousel
