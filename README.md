# kea-ddns-nsd
Take Kea IP reservations and update zonefiles for nsd to pick up local network changes

There is a python module to do this called kea-ddns-nsd.

To build and install follow PEP517...

Install the python packages "pip", "setuptools", "wheel" and "build".

To build the package...

python -m build

To install 

pip install dist/kea_ddns_nsd- ....

for some distros, e.g. gentoo; it's best to install locally to avoid conflict with the pacakge manager.

#permissions

The script creates a lock file  "/run/kea/zone_update.lock". Hence the script, run as the kea user, needs
access to this folder,

The scipt modifies the files /var/lib/nsd/home.arpa.reverse and /var/lib/nsd/home.arpa.forward

Hence the script, run as the kea user, needs access to ths folder and permission to update these files.

nsd needs to be able to read the said zone files.

Suggestion is to add the kea user group to nsd user and vice-versa, and allow group access to the zone files.

usermod -G dhcp nsd
usermod -G nsd dhcp



