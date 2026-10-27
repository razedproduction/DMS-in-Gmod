#pragma once

#include "vphysics_nm.h"

#include <cstdint>
#include <vector>

class CPhysicsNM final : public IPhysicsNM
{
public:
	bool GetBodyState( const IPhysicsObject *pObject, PhysicsNMBodyState_t *pState ) override;
	bool ApplyBodyDrive( IPhysicsObject *pObject, const float *pForce, const float *pTorque ) override;
	bool SetJointArmature( IPhysicsConstraint *pConstraint, float flArmature ) override;
	float GetJointArmature( const IPhysicsConstraint *pConstraint ) override;
	IPhysicsConstraint *FindConstraint( IPhysicsObject *pObject0, IPhysicsObject *pObject1 ) override;
	bool SetStepCallback( IPhysicsEnvironment *pEnvironment, PhysicsNMStepFn_t pCallback, void *pContext ) override;
	IPhysicsEnvironment *GetObjectEnvironment( const IPhysicsObject *pObject ) override;
	int GetSubstepCount() override;
	bool SetJointAnchor( IPhysicsConstraint *pConstraint, const float *pWorldPoint ) override;
	bool SetJointLimits( IPhysicsConstraint *pConstraint, IPhysicsObject *pParent, const float *pWorldFrames, bool bHinge, const float *pLimits ) override;
	bool SetBodyInertia( IPhysicsObject *pObject, const float *pTensor ) override;
	bool SetJointBallLimits( IPhysicsConstraint *pConstraint, IPhysicsObject *pParent, const float *pWorldFrames, const float *pLimits ) override;
	bool GetBodyInertia( const IPhysicsObject *pObject, float *pTensor ) override;
	bool SetBodyMaterial( IPhysicsObject *pObject, float flFriction, float flElasticity ) override;
	bool SetSurfaceMaterial( int nSurfaceProp, float flFriction, float flElasticity ) override;
	IPhysicsObject *CreateBody( IPhysicsEnvironment *pEnvironment, IPhysicsObject *pOwner, const float *pPoints, int nPoints, float flMass, const float *pPosition, const float *pRotation, unsigned short nGameFlags ) override;
	IPhysicsConstraint *CreateJoint( IPhysicsObject *pParent, IPhysicsObject *pChild, bool bHinge ) override;
	void DestroyJoint( IPhysicsConstraint *pConstraint ) override;
	void DestroyBody( IPhysicsObject *pObject ) override;
	bool SetBodyCollisionPairs( IPhysicsObject **ppObjects, int nObjects, const int *pPairs, int nPairs ) override;
	bool SetBodyState( IPhysicsObject *pObject, const PhysicsNMBodyState_t *pState ) override;
};

extern CPhysicsNM g_PhysicsNM;

// SetBodyCollisionPairs' set: which of its bodies collide with which (n by n, by their indices in it)
struct NMCollisionSet_t
{
	int m_nCount = 0;
	std::vector<unsigned char> m_Collide;
};

class CPhysicsObject;

// The pair's decision if both bodies are in one SetBodyCollisionPairs set: true with bCollide set, false otherwise
// (the game's filter decides). Read only, so safe from Box3D's filter threads.
bool GetNMCollisionDecision( const CPhysicsObject *pObject0, const CPhysicsObject *pObject1, bool &bCollide );

// A Box3D shape material's userMaterialId of a body SetBodyMaterial gave NM's material: this bit, with its surface
// property in the low bits.
inline constexpr uint64_t NM_BODY_MATERIAL = 1ull << 62;
inline constexpr uint64_t NM_SURFACE_MASK = 0xFFFFFFFFull;

// SetSurfaceMaterial's friction and elasticity of a surface property, false for none.
bool GetNMSurfaceMaterial( uint64_t nSurfaceProp, float &flFriction, float &flElasticity );
