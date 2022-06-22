#!/usr/bin/env python3

##
# Copyright (c) 2022 Peter Leese
#
# Licensed under the GPL License. See LICENSE file in the project root for full license information.  
##


import os
import logging


if __package__ in ("", None):
	import update_zonefiles as up_zone
else:
	from . import update_zonefiles as up_zone


def main():
	up_zone.main()


if __name__ == "__main__":
	main()
